// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod eip_1559_transaction;
mod eip_155_transaction;
mod homestead_transaction;
mod transaction;

pub use transaction::is_valid_eth_tx_for_sending;
pub type EthChainId = u64;
