// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::message::signed_message::SignedMessage;
use crate::shim::address::Protocol;
use crate::shim::crypto::{SECP_SIG_LEN, Signature, SignatureType};
use crate::shim::message::MethodNum;
use crate::shim::{address::Address, econ::TokenAmount, message::Message};
use ambassador::Delegate;
use fvm_ipld_encoding::RawBytes;
use get_size2::GetSize;
use serde::{Deserialize, Serialize};
use spire_enum::prelude::delegated_enum;
use std::sync::Arc;

/// `Enum` to encapsulate signed and unsigned messages. Useful when working with
/// both types
#[delegated_enum]
#[derive(
    Clone, Debug, Serialize, Deserialize, Hash, Eq, PartialEq, GetSize, derive_more::From, Delegate,
)]
#[delegate(MessageRead)]
#[serde(untagged)]
pub enum ChainMessage {
    Unsigned(Arc<Message>),
    Signed(Arc<SignedMessage>),
}

impl From<Message> for ChainMessage {
    fn from(msg: Message) -> Self {
        Arc::new(msg).into()
    }
}

impl From<SignedMessage> for ChainMessage {
    fn from(msg: SignedMessage) -> Self {
        Arc::new(msg).into()
    }
}

impl ChainMessage {
    pub fn message(&self) -> &Message {
        match self {
            Self::Unsigned(m) => m,
            Self::Signed(sm) => sm.message(),
        }
    }

    pub fn cid(&self) -> cid::Cid {
        delegate_chain_message!(self.cid())
    }

    /// Tests if a message is equivalent to another replacing message.
    /// A replacing message is a message with a different CID,
    /// any of Gas values, and different signature, but with all
    /// other parameters matching (source/destination, nonce, parameters, etc.)
    /// See <https://github.com/filecoin-project/lotus/blob/813d133c24295629ef442fc3aa60e6e6b2101226/chain/types/message.go#L138>
    pub fn equal_call(&self, other: &Self) -> bool {
        self.message().equal_call(other.message())
    }

    /// Wrap `msg` so its on-chain size matches a real submission — the FVM
    /// charges per-byte gas based on the wire encoding including the
    /// signature. Returns `Unsigned` for protocols that don't carry one.
    pub fn for_gas_estimation(msg: Message, from_protocol: Protocol) -> Self {
        match from_protocol {
            Protocol::Secp256k1 => {
                SignedMessage::new_unchecked(msg, Signature::new_secp256k1(vec![0; SECP_SIG_LEN]))
                    .into()
            }
            // In Lotus, delegated signatures have the same length as SECP256k1.
            // This may or may not change in the future.
            Protocol::Delegated => SignedMessage::new_unchecked(
                msg,
                Signature::new(SignatureType::Delegated, vec![0; SECP_SIG_LEN]),
            )
            .into(),
            _ => msg.into(),
        }
    }
}

impl MessageReadWrite for ChainMessage {
    fn set_gas_limit(&mut self, amount: u64) {
        delegate_chain_message!(self => |i| Arc::make_mut(i).set_gas_limit(amount))
    }

    fn set_sequence(&mut self, sequence: u64) {
        delegate_chain_message!(self => |i| Arc::make_mut(i).set_sequence(sequence))
    }

    fn set_gas_fee_cap(&mut self, cap: TokenAmount) {
        delegate_chain_message!(self => |i| Arc::make_mut(i).set_gas_fee_cap(cap))
    }

    fn set_gas_premium(&mut self, prem: TokenAmount) {
        delegate_chain_message!(self => |i| Arc::make_mut(i).set_gas_premium(prem))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_msg() -> Message {
        Message {
            from: Address::new_id(2),
            to: Address::new_id(1),
            ..Default::default()
        }
    }

    #[track_caller]
    fn assert_signed_with_zero_sig(chain_msg: ChainMessage, sig_type: SignatureType, msg: &Message) {
        let ChainMessage::Signed(signed) = chain_msg else {
            panic!("expected Signed variant");
        };
        assert_eq!(signed.signature().signature_type(), sig_type);
        assert_eq!(signed.signature().bytes(), &vec![0; SECP_SIG_LEN]);
        assert_eq!(signed.message(), msg);
    }

    #[track_caller]
    fn assert_unsigned(chain_msg: ChainMessage, msg: &Message) {
        let ChainMessage::Unsigned(m) = chain_msg else {
            panic!("expected Unsigned variant");
        };
        assert_eq!(&*m, msg);
    }

    #[test]
    fn test_for_gas_estimation_secp256k1() {
        let msg = dummy_msg();
        let chain_msg = ChainMessage::for_gas_estimation(msg.clone(), Protocol::Secp256k1);
        assert_signed_with_zero_sig(chain_msg, SignatureType::Secp256k1, &msg);
    }

    #[test]
    fn test_for_gas_estimation_delegated() {
        let msg = dummy_msg();
        let chain_msg = ChainMessage::for_gas_estimation(msg.clone(), Protocol::Delegated);
        // Lotus uses the same signature length as Secp256k1 for delegated; this
        // lock-in prevents an accidental length change from regressing parity.
        assert_signed_with_zero_sig(chain_msg, SignatureType::Delegated, &msg);
    }

    #[test]
    fn test_for_gas_estimation_bls() {
        let msg = dummy_msg();
        let chain_msg = ChainMessage::for_gas_estimation(msg.clone(), Protocol::BLS);
        assert_unsigned(chain_msg, &msg);
    }

    #[test]
    fn test_for_gas_estimation_id() {
        let msg = dummy_msg();
        let chain_msg = ChainMessage::for_gas_estimation(msg.clone(), Protocol::ID);
        assert_unsigned(chain_msg, &msg);
    }

    #[test]
    fn test_for_gas_estimation_actor() {
        let msg = dummy_msg();
        let chain_msg = ChainMessage::for_gas_estimation(msg.clone(), Protocol::Actor);
        assert_unsigned(chain_msg, &msg);
    }
}
