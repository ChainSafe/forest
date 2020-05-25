// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Merge;
use address::Address;
use clock::ChainEpoch;
use crypto::Signature;
use encoding::{serde_bytes, tuple::*};
use num_bigint::{bigint_ser, BigInt};
use vm::{MethodNum, Serialized};

/// Maximum number of lanes in a channel
pub const LANE_LIMIT: usize = 256;

// TODO replace placeholder when params finished
pub const SETTLE_DELAY: ChainEpoch = 1;

/// Constructor parameters for payment channel actor
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ConstructorParams {
    pub from: Address,
    pub to: Address,
}

/// A voucher is sent by `from` to `to` off-chain in order to enable
/// `to` to redeem payments on-chain in the future
#[derive(Default, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct SignedVoucher {
    /// Min epoch before which the voucher cannot be redeemed
    pub time_lock_min: ChainEpoch,
    /// Max epoch beyond which the voucher cannot be redeemed
    /// set to 0 means no timeout
    pub time_lock_max: ChainEpoch,
    /// (optional) Used by `to` to validate
    // TODO revisit this type, can probably be a 32 byte array
    #[serde(with = "serde_bytes")]
    pub secret_pre_image: Vec<u8>,
    /// (optional) Specified by `from` to add a verification method to the voucher
    pub extra: Option<ModVerifyParams>,
    /// Specifies which lane the Voucher merges into (will be created if does not exist)
    pub lane: u64,
    /// Set by `from` to prevent redemption of stale vouchers on a lane
    pub nonce: u64,
    /// Amount voucher can be redeemed for
    #[serde(with = "bigint_ser")]
    pub amount: BigInt,
    /// (optional) Can extend channel min_settle_height if needed
    pub min_settle_height: ChainEpoch,

    /// (optional) Set of lanes to be merged into `lane`
    pub merges: Vec<Merge>,

    /// Sender's signature over the voucher (sign on none)
    pub signature: Option<Signature>,
}

/// Modular Verification method
#[derive(Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct ModVerifyParams {
    pub actor: Address,
    pub method: MethodNum,
    pub data: Serialized,
}

/// Payment Verification parameters
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct PaymentVerifyParams {
    pub extra: Serialized,
    // TODO revisit these to see if they should be arrays or optional
    #[serde(with = "serde_bytes")]
    pub proof: Vec<u8>,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct UpdateChannelStateParams {
    pub sv: SignedVoucher,
    #[serde(with = "serde_bytes")]
    pub secret: Vec<u8>,
    #[serde(with = "serde_bytes")]
    pub proof: Vec<u8>,
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
