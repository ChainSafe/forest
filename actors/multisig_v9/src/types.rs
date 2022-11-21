// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt::Display;

use fvm_ipld_encoding::tuple::*;
use fvm_ipld_encoding::{serde_bytes, Cbor, RawBytes};
use fvm_ipld_hamt::BytesKey;
use fvm_shared::address::Address;

use fvm_shared::clock::ChainEpoch;
use fvm_shared::econ::TokenAmount;
use fvm_shared::error::ExitCode;
use fvm_shared::MethodNum;
use integer_encoding::VarInt;
use serde::{Deserialize, Serialize};

/// SignersMax is the maximum number of signers allowed in a multisig. If more
/// are required, please use a combining tree of multisigs.
pub const SIGNERS_MAX: usize = 256;

/// Transaction ID type
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, Hash, Eq, PartialEq, PartialOrd)]
#[serde(transparent)]
pub struct TxnID(pub i64);

impl TxnID {
    pub fn key(self) -> BytesKey {
        self.0.encode_var_vec().into()
    }
}

impl Display for TxnID {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Transaction type used in multisig actor
#[derive(Clone, PartialEq, Eq, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct Transaction {
    pub to: Address,
    pub value: TokenAmount,
    pub method: MethodNum,
    pub params: RawBytes,

    pub approved: Vec<Address>,
}

/// Data for a BLAKE2B-256 to be attached to methods referencing proposals via TXIDs.
/// Ensures the existence of a cryptographic reference to the original proposal. Useful
/// for offline signers and for protection when reorgs change a multisig TXID.
///
/// Requester - The requesting multisig wallet member.
/// All other fields - From the "Transaction" struct.
#[derive(Serialize_tuple, Debug)]
pub struct ProposalHashData<'a> {
    pub requester: Option<&'a Address>,
    pub to: &'a Address,
    pub value: &'a TokenAmount,
    pub method: &'a MethodNum,
    pub params: &'a RawBytes,
}

/// Constructor parameters for multisig actor.
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ConstructorParams {
    pub signers: Vec<Address>,
    pub num_approvals_threshold: u64,
    pub unlock_duration: ChainEpoch,
    // * Added in v2
    pub start_epoch: ChainEpoch,
}

/// Propose method call parameters.
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ProposeParams {
    pub to: Address,
    pub value: TokenAmount,
    pub method: MethodNum,
    pub params: RawBytes,
}

/// Propose method call return.
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ProposeReturn {
    /// TxnID is the ID of the proposed transaction.
    pub txn_id: TxnID,
    /// Applied indicates if the transaction was applied as opposed to proposed but not applied
    /// due to lack of approvals.
    pub applied: bool,
    /// Code is the exitcode of the transaction, if Applied is false this field should be ignored.
    pub code: ExitCode,
    /// Ret is the return value of the transaction, if Applied is false this field should
    /// be ignored.
    pub ret: RawBytes,
}

impl Cbor for ProposeParams {}
impl Cbor for ProposeReturn {}

/// Parameters for approve and cancel multisig functions.
#[derive(Clone, PartialEq, Eq, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct TxnIDParams {
    pub id: TxnID,
    /// Optional hash of proposal to ensure an operation can only apply to a
    /// specific proposal.
    #[serde(with = "serde_bytes")]
    pub proposal_hash: Vec<u8>,
}

/// Parameters for approve and cancel multisig functions.
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ApproveReturn {
    /// Applied indicates if the transaction was applied as opposed to proposed but not applied
    /// due to lack of approvals
    pub applied: bool,
    /// Code is the exitcode of the transaction, if Applied is false this field should be ignored.
    pub code: ExitCode,
    /// Ret is the return value of the transaction, if Applied is false this field should
    /// be ignored.
    pub ret: RawBytes,
}

impl Cbor for TxnIDParams {}
impl Cbor for ApproveReturn {}

/// Add signer params.
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct AddSignerParams {
    pub signer: Address,
    pub increase: bool,
}

/// Remove signer params.
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct RemoveSignerParams {
    pub signer: Address,
    pub decrease: bool,
}

impl Cbor for RemoveSignerParams {}

/// Swap signer multisig method params
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct SwapSignerParams {
    pub from: Address,
    pub to: Address,
}

impl Cbor for SwapSignerParams {}

/// Propose method call parameters
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ChangeNumApprovalsThresholdParams {
    pub new_threshold: u64,
}

/// Lock balance call params.
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct LockBalanceParams {
    pub start_epoch: ChainEpoch,
    pub unlock_duration: ChainEpoch,
    pub amount: TokenAmount,
}
