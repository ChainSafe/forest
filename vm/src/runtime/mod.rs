#![allow(dead_code)]
mod input;
mod output;

pub use self::input::*;
pub use self::output::*;

use super::message::Message;
use super::{ExitCode, TokenAmount};

use address::Address;
use cid::Cid;
use crypto::Signature;
use std::any::Any;

// TODO: ref #64
pub struct ChainEpoch;
pub struct Randomness; // TODO
pub struct CallerPattern; // TODO
pub struct ActorStateHandle; // TODO
pub struct IPLDObject; // TODO
pub struct ComputeFunctionID; // TODO

/// Runtime is the VM's internal runtime object.
/// this is everything that is accessible to actors, beyond parameters.
pub trait Runtime {
    /// Retrieves current epoch
    fn curr_epoch(&self) -> ChainEpoch; // TODO define epoch

    /// Randomness returns a (pseudo)random stream (indexed by offset) for the current epoch.
    // TODO define randomness/epoch variable
    fn randomness(&self, epoch: ChainEpoch, offset: u64) -> Randomness;

    /// Not necessarily the actor in the From field of the initial on-chain Message.
    fn immediate_caller(&self) -> Address;
    fn validate_immediate_caller_is(&self, caller: Address);
    fn validate_immediate_caller_accept_any(&self);
    fn validate_immediate_caller_matches(&self, caller_pattern: CallerPattern); // TODO add caller pattern

    /// The address of the actor receiving the message.
    fn curr_receiver(&self) -> Address;

    /// The actor who mined the block in which the initial on-chain message appears.
    fn top_level_block_winner(&self) -> Address;

    fn acquire_state(&self) -> ActorStateHandle; // TODO add actor state handle

    /// Return successfully from invocation.
    fn success_return(&self) -> InvocOutput;
    /// Return from invocation with a value.
    fn value_return(&self, bytes: Vec<u8>) -> InvocOutput;

    /// Throw an error indicating a failure condition has occurred, from which the given actor
    /// code is unable to recover.
    // TODO determine if abort happens here
    // fn abort(&self, err_exit_code: ExitCode, msg: String);

    /// Calls Abort with InvalidArguments error.
    fn abort_arg_msg(&self, msg: String);
    fn abort_arg(&self);

    /// Calls Abort with InconsistentState error.
    fn abort_state_msg(&self, msg: String);
    fn abort_state(&self);

    /// Calls Abort with InsufficientFunds error.
    fn abort_funds_msg(&self, msg: String);
    fn abort_funds(&self);

    /// Calls Abort with RuntimeAPIError.
    /// For internal use only (not in actor code).
    fn abort_api(&self, msg: String);

    /// Check that the given condition is true (and call Abort if not).
    fn assert(&self, cond: bool);

    /// Retrieves current balance in VM.
    fn current_balance(&self) -> TokenAmount;
    /// Retrieves value received in VM.
    fn value_received(&self) -> TokenAmount;

    fn verify_signature(&self, signer_actor: Address, sig: Signature, m: Message) -> bool;

    /// Run a (pure function) computation, consuming the gas cost associated with that function.
    /// This mechanism is intended to capture the notion of an ABI between the VM and native
    /// functions, and should be used for any function whose computation is expensive.
    fn compute(&self, id: ComputeFunctionID, args: dyn Any) -> dyn Any; // TODO define parameters

    /// Send allows the current execution context to invoke methods on other actors in the system.
    // TODO determine if both are needed in our impl
    fn send_propagating_errors(&self, input: InvocInput) -> InvocOutput;
    fn send_catching_errors(&self, input: InvocInput) -> Result<InvocOutput, ExitCode>;

    /// Computes an address for a new actor. The returned address is intended to uniquely refer
    /// to the actor even in the event of a chain re-org (whereas an ID-address might refer to a
    /// different actor after messages are re-ordered).
    fn new_actor_address(&self) -> Address;

    /// Create an actor in the state tree. May only be called by InitActor.
    fn create_actor(
        &self,
        state_cid: Cid,
        a: Address,
        init_balance: TokenAmount,
        constructor_params: dyn Any, // TODO define params
    );

    fn ipld_get(&self, c: Cid) -> Result<Vec<u8>, String>; // TODO add error type
    fn ipld_put(&self, object: IPLDObject) -> Cid; // TODO define IPLD object
}
