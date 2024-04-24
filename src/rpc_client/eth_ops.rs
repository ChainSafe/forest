// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{ApiInfo, RpcRequest};
use crate::rpc::eth::*;

impl ApiInfo {
    pub fn eth_accounts_req() -> RpcRequest<Vec<String>> {
        RpcRequest::new_v1(ETH_ACCOUNTS, ())
    }

    pub fn eth_block_number_req() -> RpcRequest<String> {
        RpcRequest::new_v1(ETH_BLOCK_NUMBER, ())
    }

    pub fn eth_get_block_by_number_req(
        block_param: BlockNumberOrHash,
        full_tx_info: bool,
    ) -> RpcRequest<Block> {
        RpcRequest::new_v1(ETH_GET_BLOCK_BY_NUMBER, (block_param, full_tx_info))
    }

    pub fn eth_chain_id_req() -> RpcRequest<String> {
        RpcRequest::new_v1(ETH_CHAIN_ID, ())
    }

    pub fn eth_gas_price_req() -> RpcRequest<String> {
        RpcRequest::new_v1(ETH_GAS_PRICE, ())
    }

    pub fn eth_get_balance_req(
        address: Address,
        block_param: BlockNumberOrHash,
    ) -> RpcRequest<BigInt> {
        RpcRequest::new_v1(ETH_GET_BALANCE, (address, block_param))
    }

    pub fn eth_subscribe_req(event: serde_json::Value) -> RpcRequest<SubscriptionID> {
        RpcRequest::new_v1(ETH_SUBSCRIBE, event)
    }

    pub fn web3_client_version_req() -> RpcRequest<String> {
        RpcRequest::new_v1(WEB3_CLIENT_VERSION, ())
    }
}
