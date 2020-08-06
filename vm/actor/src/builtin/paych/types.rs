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
#[derive(Debug, Clone, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct SignedVoucher {
    /// ChannelAddr is the address of the payment channel this signed voucher is valid for
    pub channel_addr: Address,
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
#[derive(Debug, Clone, PartialEq, Serialize_tuple, Deserialize_tuple)]
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

impl From<SignedVoucher> for UpdateChannelStateParams {
    fn from(sv: SignedVoucher) -> Self {
        UpdateChannelStateParams {
            proof: vec![],
            secret: vec![],
            sv,
        }
    }
}
