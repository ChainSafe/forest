// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::BytesKey;
use address::Address;
use clock::ChainEpoch;
use encoding::tuple::*;
use num_bigint::biguint_ser;
use num_bigint::BigInt;
use serde::{Deserialize, Serialize};
use vm::{MethodNum, Serialized, TokenAmount};

/// Transaction ID type
// TODO change to uvarint encoding
#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TxnID(pub i64);

impl TxnID {
    pub fn key(self) -> BytesKey {
        let v = BigInt::from(self.0 as i64);
        BytesKey(v.to_signed_bytes_be())
    }
}

#[derive(Clone, PartialEq, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct ProposalHashData {
    pub requester: Address,
    pub to: Address,
    #[serde(with = "biguint_ser")]
    pub value: TokenAmount,
    pub method: u64,
    pub params: Vec<u8>,
}

/// Transaction type used in multisig actor
#[derive(Clone, PartialEq, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct Transaction {
    pub to: Address,
    #[serde(with = "biguint_ser")]
    pub value: TokenAmount,
    pub method: MethodNum,
    pub params: Serialized,

    pub approved: Vec<Address>,
}

/// Constructor parameters for multisig actor
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ConstructorParams {
    pub signers: Vec<Address>,
    pub num_approvals_threshold: i64,
    pub unlock_duration: ChainEpoch,
}

/// Propose method call parameters
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ProposeParams {
    pub to: Address,
    #[serde(with = "biguint_ser")]
    pub value: TokenAmount,
    pub method: MethodNum,
    pub params: Serialized,
}

/// Propose method call parameters
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct TxnIDParams {
    pub id: TxnID,
    #[serde(with = "serde_bytes")]
    pub proposal_hash: [u8; 32],
}

/// Add signer params
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct AddSignerParams {
    pub signer: Address,
    pub increase: bool,
}

/// Remove signer params
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct RemoveSignerParams {
    pub signer: Address,
    pub decrease: bool,
}

/// Swap signer multisig method params
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct SwapSignerParams {
    pub from: Address,
    pub to: Address,
}

/// Propose method call parameters
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ChangeNumApprovalsThresholdParams {
    pub new_threshold: i64,
}
