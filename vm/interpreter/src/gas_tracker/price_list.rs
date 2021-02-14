// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::GasCharge;
use ahash::AHashMap;
use clock::ChainEpoch;
use crypto::SignatureType;
use fil_types::{
    PieceInfo, RegisteredPoStProof, RegisteredSealProof, SealVerifyInfo, WindowPoStVerifyInfo,
};
use networks::UPGRADE_CALICO_HEIGHT;
use num_traits::Zero;
use vm::{MethodNum, TokenAmount, METHOD_SEND};

lazy_static! {
    static ref BASE_PRICES: PriceList = PriceList {
        compute_gas_multiplier: 1,
        storage_gas_multiplier: 1000,

        on_chain_message_compute_base: 38863,
        on_chain_message_storage_base: 36,
        on_chain_message_storage_per_byte: 1,

        on_chain_return_value_per_byte: 1,

        send_base: 29233,
        send_transfer_funds: 27500,
        send_transfer_only_premium: 159672,
        send_invoke_method: -5377,

        ipld_get_base: 75242,
        ipld_put_base: 84070,
        ipld_put_per_byte: 1,

        create_actor_compute: 1108454,
        create_actor_storage: 36 + 40,
        delete_actor: -(36 + 40),

        bls_sig_cost: 16598605,
        secp256k1_sig_cost: 1637292,

        hashing_base: 31355,
        compute_unsealed_sector_cid_base: 98647,
        verify_seal_base: 2000, // TODO revisit potential removal of this
        verify_consensus_fault: 495422,

        verify_post_lookup: [
            (
                RegisteredPoStProof::StackedDRGWindow512MiBV1,
                ScalingCost {
                    flat: 123861062,
                    scale: 9226981,
                },
            ),
            (
                RegisteredPoStProof::StackedDRGWindow32GiBV1,
                ScalingCost {
                    flat: 748593537,
                    scale: 85639,
                },
            ),
            (
                RegisteredPoStProof::StackedDRGWindow64GiBV1,
                ScalingCost {
                    flat: 748593537,
                    scale: 85639,
                },
            ),
        ]
        .iter()
        .copied()
        .collect(),
        verify_post_discount: true,
    };

    static ref CALICO_PRICES: PriceList = PriceList {
        compute_gas_multiplier: 1,
        storage_gas_multiplier: 1300,

        on_chain_message_compute_base: 38863,
        on_chain_message_storage_base: 36,
        on_chain_message_storage_per_byte: 1,

        on_chain_return_value_per_byte: 1,

        send_base: 29233,
        send_transfer_funds: 27500,
        send_transfer_only_premium: 159672,
        send_invoke_method: -5377,

        ipld_get_base: 114617,
        ipld_put_base: 353640,
        ipld_put_per_byte: 1,

        create_actor_compute: 1108454,
        create_actor_storage: 36 + 40,
        delete_actor: -(36 + 40),

        bls_sig_cost: 16598605,
        secp256k1_sig_cost: 1637292,

        hashing_base: 31355,
        compute_unsealed_sector_cid_base: 98647,
        verify_seal_base: 2000, // TODO revisit potential removal of this
        verify_consensus_fault: 495422,

        verify_post_lookup: [
            (
                RegisteredPoStProof::StackedDRGWindow512MiBV1,
                ScalingCost {
                    flat: 117680921,
                    scale: 43780,
                },
            ),
            (
                RegisteredPoStProof::StackedDRGWindow32GiBV1,
                ScalingCost {
                    flat: 117680921,
                    scale: 43780,
                },
            ),
            (
                RegisteredPoStProof::StackedDRGWindow64GiBV1,
                ScalingCost {
                    flat: 117680921,
                    scale: 43780,
                },
            ),
        ]
        .iter()
        .copied()
        .collect(),
        verify_post_discount: false,
    };
}

#[derive(Clone, Debug, Copy)]
pub(crate) struct ScalingCost {
    flat: i64,
    scale: i64,
}

/// Provides prices for operations in the VM
#[derive(Clone, Debug)]
pub struct PriceList {
    /// Compute gas charge multiplier
    // * This multiplier is not currently applied to anything, but is matching lotus.
    // * If the possible values are non 1 or if Lotus adds, we should change also.
    pub(crate) compute_gas_multiplier: i64,
    /// Storage gas charge multiplier
    pub(crate) storage_gas_multiplier: i64,

    /// Gas cost charged to the originator of an on-chain message (regardless of
    /// whether it succeeds or fails in application) is given by:
    ///   OnChainMessageBase + len(serialized message)*OnChainMessagePerByte
    /// Together, these account for the cost of message propagation and validation,
    /// up to but excluding any actual processing by the VM.
    /// This is the cost a block producer burns when including an invalid message.
    pub(crate) on_chain_message_compute_base: i64,
    pub(crate) on_chain_message_storage_base: i64,
    pub(crate) on_chain_message_storage_per_byte: i64,

    /// Gas cost charged to the originator of a non-nil return value produced
    /// by an on-chain message is given by:
    ///   len(return value)*OnChainReturnValuePerByte
    pub(crate) on_chain_return_value_per_byte: i64,

    /// Gas cost for any message send execution(including the top-level one
    /// initiated by an on-chain message).
    /// This accounts for the cost of loading sender and receiver actors and
    /// (for top-level messages) incrementing the sender's sequence number.
    /// Load and store of actor sub-state is charged separately.
    pub(crate) send_base: i64,

    /// Gas cost charged, in addition to SendBase, if a message send
    /// is accompanied by any nonzero currency amount.
    /// Accounts for writing receiver's new balance (the sender's state is
    /// already accounted for).
    pub(crate) send_transfer_funds: i64,

    /// Gas cost charged, in addition to SendBase, if message only transfers funds.
    pub(crate) send_transfer_only_premium: i64,

    /// Gas cost charged, in addition to SendBase, if a message invokes
    /// a method on the receiver.
    /// Accounts for the cost of loading receiver code and method dispatch.
    pub(crate) send_invoke_method: i64,

    /// Gas cost (Base + len*PerByte) for any Get operation to the IPLD store
    /// in the runtime VM context.
    pub(crate) ipld_get_base: i64,

    /// Gas cost (Base + len*PerByte) for any Put operation to the IPLD store
    /// in the runtime VM context.
    /// Note: these costs should be significantly higher than the costs for Get
    /// operations, since they reflect not only serialization/deserialization
    /// but also persistent storage of chain data.
    pub(crate) ipld_put_base: i64,
    pub(crate) ipld_put_per_byte: i64,

    /// Gas cost for creating a new actor (via InitActor's Exec method).
    /// Note: this costs assume that the extra will be partially or totally refunded while
    /// the base is covering for the put.
    pub(crate) create_actor_compute: i64,
    pub(crate) create_actor_storage: i64,

    /// Gas cost for deleting an actor.
    /// Note: this partially refunds the create cost to incentivise the deletion of the actors.
    pub(crate) delete_actor: i64,

    /// Gas cost for verifying bls signature
    pub(crate) bls_sig_cost: i64,
    /// Gas cost for verifying secp256k1 signature
    pub(crate) secp256k1_sig_cost: i64,

    pub(crate) hashing_base: i64,

    pub(crate) compute_unsealed_sector_cid_base: i64,
    pub(crate) verify_seal_base: i64,
    pub(crate) verify_post_lookup: AHashMap<RegisteredPoStProof, ScalingCost>,
    pub(crate) verify_post_discount: bool,
    pub(crate) verify_consensus_fault: i64,
}

impl PriceList {
    /// Returns the gas required for storing a message of a given size in the chain.
    #[inline]
    pub fn on_chain_message(&self, msg_size: usize) -> GasCharge {
        GasCharge::new(
            "OnChainMessage",
            self.on_chain_message_compute_base,
            (self.on_chain_message_storage_base
                + self.on_chain_message_storage_per_byte * msg_size as i64)
                * self.storage_gas_multiplier,
        )
    }
    /// Returns the gas required for storing the response of a message in the chain.
    #[inline]
    pub fn on_chain_return_value(&self, data_size: usize) -> GasCharge {
        GasCharge::new(
            "OnChainReturnValue",
            0,
            data_size as i64 * self.on_chain_return_value_per_byte * self.storage_gas_multiplier,
        )
    }
    /// Returns the gas required when invoking a method.
    #[inline]
    pub fn on_method_invocation(&self, value: &TokenAmount, method_num: MethodNum) -> GasCharge {
        let mut ret = self.send_base;
        if value != &TokenAmount::zero() {
            ret += self.send_transfer_funds;
            if method_num == METHOD_SEND {
                ret += self.send_transfer_only_premium;
            }
        }
        if method_num != METHOD_SEND {
            ret += self.send_invoke_method;
        }
        GasCharge::new("OnMethodInvocation", ret, 0)
    }
    /// Returns the gas required for storing an object.
    #[inline]
    pub fn on_ipld_get(&self) -> GasCharge {
        GasCharge::new("OnIpldGet", self.ipld_get_base, 0)
    }
    /// Returns the gas required for storing an object.
    #[inline]
    pub fn on_ipld_put(&self, data_size: usize) -> GasCharge {
        GasCharge::new(
            "OnIpldPut",
            self.ipld_put_base,
            data_size as i64 * self.ipld_put_per_byte * self.storage_gas_multiplier,
        )
    }
    /// Returns the gas required for creating an actor.
    #[inline]
    pub fn on_create_actor(&self) -> GasCharge {
        GasCharge::new(
            "OnCreateActor",
            self.create_actor_compute,
            self.create_actor_storage * self.storage_gas_multiplier,
        )
    }
    /// Returns the gas required for deleting an actor.
    #[inline]
    pub fn on_delete_actor(&self) -> GasCharge {
        GasCharge::new(
            "OnDeleteActor",
            0,
            self.delete_actor * self.storage_gas_multiplier,
        )
    }
    /// Returns gas required for signature verification.
    #[inline]
    pub fn on_verify_signature(&self, sig_type: SignatureType) -> GasCharge {
        let val = match sig_type {
            SignatureType::BLS => self.bls_sig_cost,
            SignatureType::Secp256k1 => self.secp256k1_sig_cost,
        };
        GasCharge::new("OnVerifySignature", val, 0)
    }
    /// Returns gas required for hashing data.
    #[inline]
    pub fn on_hashing(&self, _: usize) -> GasCharge {
        GasCharge::new("OnHashing", self.hashing_base, 0)
    }
    /// Returns gas required for computing unsealed sector Cid.
    #[inline]
    pub fn on_compute_unsealed_sector_cid(
        &self,
        _proof: RegisteredSealProof,
        _pieces: &[PieceInfo],
    ) -> GasCharge {
        GasCharge::new(
            "OnComputeUnsealedSectorCid",
            self.compute_unsealed_sector_cid_base,
            0,
        )
    }
    /// Returns gas required for seal verification.
    #[inline]
    pub fn on_verify_seal(&self, _info: &SealVerifyInfo) -> GasCharge {
        GasCharge::new("OnVerifySeal", self.verify_seal_base, 0)
    }
    /// Returns gas required for PoSt verification.
    #[inline]
    pub fn on_verify_post(&self, info: &WindowPoStVerifyInfo) -> GasCharge {
        let p_proof = info
            .proofs
            .first()
            .map(|p| p.post_proof)
            .unwrap_or(RegisteredPoStProof::StackedDRGWindow512MiBV1);
        let cost = self.verify_post_lookup.get(&p_proof).unwrap_or_else(|| {
            self.verify_post_lookup
                .get(&RegisteredPoStProof::StackedDRGWindow512MiBV1)
                .expect("512MiB lookup must exist in price table")
        });

        let mut gas_used = cost.flat + info.challenged_sectors.len() as i64 * cost.scale;
        if self.verify_post_discount {
            gas_used /= 2;
        }

        GasCharge::new("OnVerifyPost", gas_used, 0)
    }
    /// Returns gas required for verifying consensus fault.
    #[inline]
    pub fn on_verify_consensus_fault(&self) -> GasCharge {
        GasCharge::new("OnVerifyConsensusFault", self.verify_consensus_fault, 0)
    }
}

/// Returns gas price list by Epoch for gas consumption.
pub fn price_list_by_epoch(epoch: ChainEpoch) -> PriceList {
    if epoch < UPGRADE_CALICO_HEIGHT {
        BASE_PRICES.clone()
    } else {
        CALICO_PRICES.clone()
    }
}
