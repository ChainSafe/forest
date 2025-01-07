// Copyright 2019-2025 ChainSafe Systems
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
pub const EIP_2930_TX_TYPE: u8 = 1;
pub const EIP_1559_TX_TYPE: u8 = 2;
pub const LEGACY_V_VALUE_27: u64 = 27;
pub const LEGACY_V_VALUE_28: u64 = 28;

pub const ETH_LEGACY_HOMESTEAD_TX_CHAIN_ID: u64 = 0;

/// From Lotus:
/// > Research into Filecoin chain behavior suggests that probabilistic finality
/// > generally approaches the intended stability guarantee at, or near, 30 epochs.
/// > Although a strictly "finalized" safe recommendation remains 900 epochs.
/// > See <https://github.com/filecoin-project/FIPs/blob/master/FRCs/frc-0089.md>
pub const SAFE_EPOCH_DELAY: i64 = 30;
