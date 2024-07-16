// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod eip_1559_transaction;
mod eip_155_transaction;
mod homestead_transaction;
mod transaction;

pub use eip_1559_transaction::*;
pub use eip_155_transaction::*;
pub use homestead_transaction::*;
pub use transaction::*;
pub type EthChainId = u64;

use crate::{
    rpc::eth::types::EthAddress,
    shim::{
        crypto::{Signature, SignatureType},
        message::Message,
    },
};

/// Ethereum Improvement Proposals 1559 transaction type. This EIP changed Ethereum fee market mechanism.
/// Transaction type can have 3 distinct values:
/// - 0 for legacy transactions
/// - 1 for transactions introduced in EIP-2930
/// - 2 for transactions introduced in EIP-1559
pub const EIP_LEGACY_TX_TYPE: u64 = 0;
pub const EIP_1559_TX_TYPE: u64 = 2;

pub const ETH_LEGACY_HOMESTEAD_TX_CHAIN_ID: u64 = 0;
