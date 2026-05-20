// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::message::signed_message::SignedMessage;
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
