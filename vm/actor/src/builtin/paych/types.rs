// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Merge;
use address::Address;
use clock::ChainEpoch;
use crypto::Signature;
use encoding::{BytesDe, BytesSer};
use num_bigint::{
    bigint_ser::{BigIntDe, BigIntSer},
    BigInt,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vm::{MethodNum, Serialized};

/// Maximum number of lanes in a channel
pub const LANE_LIMIT: usize = 256;

// TODO replace placeholder when params finished
pub const SETTLE_DELAY: ChainEpoch = 1;

/// Constructor parameters for payment channel actor
pub struct ConstructorParams {
    pub from: Address,
    pub to: Address,
}

impl Serialize for ConstructorParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.from, &self.to).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ConstructorParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (from, to) = Deserialize::deserialize(deserializer)?;
        Ok(Self { from, to })
    }
}

/// A voucher is sent by `from` to `to` off-chain in order to enable
/// `to` to redeem payments on-chain in the future
#[derive(Default, Debug, PartialEq)]
pub struct SignedVoucher {
    /// Min epoch before which the voucher cannot be redeemed
    pub time_lock_min: ChainEpoch,
    /// Max epoch beyond which the voucher cannot be redeemed
    /// set to 0 means no timeout
    pub time_lock_max: ChainEpoch,
    /// (optional) Used by `to` to validate
    // TODO revisit this type, can probably be a 32 byte array
    pub secret_pre_image: Vec<u8>,
    /// (optional) Specified by `from` to add a verification method to the voucher
    pub extra: Option<ModVerifyParams>,
    /// Specifies which lane the Voucher merges into (will be created if does not exist)
    pub lane: u64,
    /// Set by `from` to prevent redemption of stale vouchers on a lane
    pub nonce: u64,
    /// Amount voucher can be redeemed for
    pub amount: BigInt,
    /// (optional) Can extend channel min_settle_height if needed
    pub min_settle_height: ChainEpoch,

    /// (optional) Set of lanes to be merged into `lane`
    pub merges: Vec<Merge>,

    /// Sender's signature over the voucher (sign on none)
    pub signature: Option<Signature>,
}

impl Serialize for SignedVoucher {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.time_lock_min,
            &self.time_lock_max,
            BytesSer(&self.secret_pre_image),
            &self.extra,
            &self.lane,
            &self.nonce,
            BigIntSer(&self.amount),
            &self.min_settle_height,
            &self.merges,
            &self.signature,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SignedVoucher {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (
            time_lock_min,
            time_lock_max,
            BytesDe(secret_pre_image),
            extra,
            lane,
            nonce,
            BigIntDe(amount),
            min_settle_height,
            merges,
            signature,
        ) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            time_lock_min,
            time_lock_max,
            secret_pre_image,
            extra,
            lane,
            nonce,
            amount,
            min_settle_height,
            merges,
            signature,
        })
    }
}

/// Modular Verification method
#[derive(Default, Debug, PartialEq)]
pub struct ModVerifyParams {
    pub actor: Address,
    pub method: MethodNum,
    pub data: Serialized,
}

impl Serialize for ModVerifyParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.actor, &self.method, &self.data).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ModVerifyParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (actor, method, data) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            actor,
            method,
            data,
        })
    }
}

/// Payment Verification parameters
pub struct PaymentVerifyParams {
    pub extra: Serialized,
    // TODO revisit these to see if they should be arrays or optional
    pub proof: Vec<u8>,
}

impl Serialize for PaymentVerifyParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.extra, BytesSer(&self.proof)).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PaymentVerifyParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (extra, BytesDe(proof)) = Deserialize::deserialize(deserializer)?;
        Ok(Self { extra, proof })
    }
}

pub struct UpdateChannelStateParams {
    pub sv: SignedVoucher,
    pub secret: Vec<u8>,
    pub proof: Vec<u8>,
}

impl Serialize for UpdateChannelStateParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.sv, BytesSer(&self.secret), BytesSer(&self.proof)).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for UpdateChannelStateParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (sv, BytesDe(secret), BytesDe(proof)) = Deserialize::deserialize(deserializer)?;
        Ok(Self { sv, secret, proof })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use encoding::{from_slice, to_vec};

    #[test]
    fn signed_voucher_serialize_optional_unset() {
        let v = SignedVoucher {
            time_lock_min: 1,
            time_lock_max: 2,
            lane: 3,
            nonce: 4,
            amount: BigInt::from(5),
            signature: Some(Signature::new_bls(b"doesn't matter".to_vec())),
            ..Default::default()
        };
        let bz = to_vec(&v).unwrap();
        assert_eq!(
            hex::encode(&bz),
            hex::encode(
                &hex::decode("8a010240f6030442000500804f02646f65736e2774206d6174746572").unwrap()
            )
        );
        assert_eq!(from_slice::<SignedVoucher>(&bz).unwrap(), v);
    }
}
