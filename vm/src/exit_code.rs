// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use encoding::repr::*;
use num_derive::FromPrimitive;

/// ExitCode defines the exit code from the VM execution
#[repr(u64)]
#[derive(PartialEq, Eq, Debug, Clone, Copy, FromPrimitive, Serialize_repr, Deserialize_repr)]
pub enum ExitCode {
    Ok = 0,

    /// Indicates failure to find an actor in the state tree.
    SysErrSenderInvalid = 1,

    /// Indicates failure to find the code for an actor.
    SysErrSenderStateInvalid = 2,

    /// Indicates failure to find a method in an actor.
    SysErrInvalidMethod = 3,

    /// Indicates syntactically invalid parameters for a method.
    SysErrInvalidParameters = 4,

    /// Indicates a message sender has insufficient funds for a message's execution.
    SysErrInvalidReceiver = 5,

    /// Indicates a message invocation out of sequence.
    SysErrInsufficientFunds = 6,

    /// Indicates message execution (including subcalls) used more gas than the specified limit.
    SysErrOutOfGas = 7,

    /// Indicates a message execution is forbidden for the caller.
    SysErrForbidden = 8,

    /// Indicates actor code performed a disallowed operation. Disallowed operations include:
    /// - mutating state outside of a state acquisition block
    /// - failing to invoke caller validation
    /// - aborting with a reserved exit code (including success or a system error).
    SysErrorIllegalActor = 9,

    /// Indicates an invalid argument passed to a runtime method.
    SysErrorIllegalArgument = 10,

    /// Indicates  an object failed to de/serialize for storage.
    SysErrSerialization = 11,

    /// Reserved exit codes, do not use.
    SysErrorReserved1 = 12,
    SysErrorReserved2 = 13,
    SysErrorReserved3 = 14,

    /// Indicates something broken within the VM.
    SysErrInternal = 15,

    // -------Actor Error Codes-------
    /// Indicates a method parameter is invalid.
    ErrIllegalArgument = 16,
    /// Indicates a requested resource does not exist.
    ErrNotFound = 17,
    /// Indicates an action is disallowed.
    ErrForbidden = 18,
    /// Indicates a balance of funds is insufficient.
    ErrInsufficientFunds = 19,
    /// Indicates an actor's internal state is invalid.
    ErrIllegalState = 20,
    /// Indicates de/serialization failure within actor code.
    ErrSerialization = 21,
    /// Power actor specific exit code.
    // * remove this and support custom codes if there is overlap on actor specific codes in future
    ErrTooManyProveCommits = 32,

    ErrPlaceholder = 1000,
}

impl ExitCode {
    /// returns true if the exit code was a success
    pub fn is_success(self) -> bool {
        matches!(self, ExitCode::Ok)
    }
}
