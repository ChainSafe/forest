// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::BytesKey;
use address::Address;
use clock::ChainEpoch;
use num_bigint::biguint_ser::{BigUintDe, BigUintSer};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vm::{MethodNum, Serialized, TokenAmount};

/// Transaction ID type
// TODO change to uvarint encoding
#[derive(Clone, Copy, Default)]
pub struct TxnID(pub i64);

impl TxnID {
    pub fn key(self) -> BytesKey {
        // TODO
        todo!();
    }
}

impl Serialize for TxnID {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TxnID {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(TxnID(Deserialize::deserialize(deserializer)?))
    }
}

/// Transaction type used in multisig actor
#[derive(Clone, PartialEq, Debug)]
pub struct Transaction {
    pub to: Address,
    pub value: TokenAmount,
    pub method: MethodNum,
    pub params: Serialized,

    pub approved: Vec<Address>,
}

impl Serialize for Transaction {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.to,
            BigUintSer(&self.value),
            &self.method,
            &self.params,
            &self.approved,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Transaction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (to, BigUintDe(value), method, params, approved) =
            Deserialize::deserialize(deserializer)?;
        Ok(Self {
            to,
            value,
            method,
            params,
            approved,
        })
    }
}

/// Constructor parameters for multisig actor
pub struct ConstructorParams {
    pub signers: Vec<Address>,
    pub num_approvals_threshold: i64,
    pub unlock_duration: ChainEpoch,
}

impl Serialize for ConstructorParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.signers,
            &self.num_approvals_threshold,
            &self.unlock_duration,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ConstructorParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (signers, num_approvals_threshold, unlock_duration) =
            Deserialize::deserialize(deserializer)?;
        Ok(Self {
            signers,
            num_approvals_threshold,
            unlock_duration,
        })
    }
}

/// Propose method call parameters
pub struct ProposeParams {
    pub to: Address,
    pub value: TokenAmount,
    pub method: MethodNum,
    pub params: Serialized,
}

impl Serialize for ProposeParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.to,
            BigUintSer(&self.value),
            &self.method,
            &self.params,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ProposeParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (to, BigUintDe(value), method, params) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            to,
            value,
            method,
            params,
        })
    }
}

/// Propose method call parameters
pub struct TxnIDParams {
    pub id: TxnID,
}

impl Serialize for TxnIDParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        [&self.id].serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TxnIDParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let [id]: [TxnID; 1] = Deserialize::deserialize(deserializer)?;
        Ok(Self { id })
    }
}

/// Add signer params
pub struct AddSignerParams {
    pub signer: Address,
    pub increase: bool,
}

impl Serialize for AddSignerParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.signer, &self.increase).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AddSignerParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (signer, increase) = Deserialize::deserialize(deserializer)?;
        Ok(Self { signer, increase })
    }
}

/// Remove signer params
pub struct RemoveSignerParams {
    pub signer: Address,
    pub decrease: bool,
}

impl Serialize for RemoveSignerParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.signer, &self.decrease).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RemoveSignerParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (signer, decrease) = Deserialize::deserialize(deserializer)?;
        Ok(Self { signer, decrease })
    }
}

/// Swap signer multisig method params
pub struct SwapSignerParams {
    pub from: Address,
    pub to: Address,
}

impl Serialize for SwapSignerParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.from, &self.to).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SwapSignerParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (from, to) = Deserialize::deserialize(deserializer)?;
        Ok(Self { from, to })
    }
}

/// Propose method call parameters
pub struct ChangeNumApprovalsThresholdParams {
    pub new_threshold: i64,
}

impl Serialize for ChangeNumApprovalsThresholdParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        [&self.new_threshold].serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ChangeNumApprovalsThresholdParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let [new_threshold]: [i64; 1] = Deserialize::deserialize(deserializer)?;
        Ok(Self { new_threshold })
    }
}
