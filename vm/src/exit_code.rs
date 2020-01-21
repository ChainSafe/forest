// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

/// ExitCode defines the exit code from the VM execution
#[derive(PartialEq, Eq, Debug, Clone)]
pub enum ExitCode {
    /// Code for successful VM execution
    Success,
    /// VM execution failed with system error
    SystemErrorCode(SysCode),
    /// VM execution failed with a user code
    UserDefinedError(UserCode),
}

/// Defines the system error codes defined by the protocol
#[derive(PartialEq, Eq, Debug, Clone)]
pub enum SysCode {
    /// ActorNotFound represents a failure to find an actor.
    ActorNotFound,

    /// ActorCodeNotFound represents a failure to find the code for a
    /// particular actor in the VM registry.
    ActorCodeNotFound,

    /// InvalidMethod represents a failure to find a method in
    /// an actor
    InvalidMethod,

    /// InvalidArguments indicates that a method was called with the incorrect
    /// number of arguments, or that its arguments did not satisfy its
    /// preconditions
    InvalidArguments,

    /// InsufficientFunds represents a failure to apply a message, as
    /// it did not carry sufficient funds for its application.
    InsufficientFunds,

    /// InvalidCallSeqNum represents a message invocation out of sequence.
    /// This happens when message.CallSeqNum is not exactly actor.CallSeqNum + 1
    InvalidCallSeqNum,

    /// OutOfGas is returned when the execution of an actor method
    /// (including its subcalls) uses more gas than initially allocated.
    OutOfGas,

    /// RuntimeAPIError is returned when an actor method invocation makes a call
    /// to the runtime that does not satisfy its preconditions.
    RuntimeAPIError,

    /// RuntimeAssertFailure is returned when an actor method invocation calls
    /// rt.Assert with a false condition.
    RuntimeAssertFailure,

    /// MethodSubcallError is returned when an actor method's Send call has
    /// returned with a failure error code (and the Send call did not specify
    /// to ignore errors).
    MethodSubcallError,
}

/// defines user specific error codes from VM execution
#[derive(PartialEq, Eq, Debug, Clone)]
pub enum UserCode {
    InsufficientFunds,
    InvalidArguments,
    InconsistentState,

    InvalidSectorPacking,
    SealVerificationFailed,
    DeadlineExceeded,
    InsufficientPledgeCollateral,
}

impl ExitCode {
    /// returns true if the exit code was a success
    pub fn is_success(&self) -> bool {
        match self {
            ExitCode::Success => true,
            _ => false,
        }
    }
    /// returns true if exited with an error code
    pub fn is_error(&self) -> bool {
        match self {
            ExitCode::Success => false,
            _ => true,
        }
    }
    /// returns true if the execution was successful
    pub fn allows_state_update(&self) -> bool {
        match self {
            ExitCode::Success => true,
            _ => false,
        }
    }
}
