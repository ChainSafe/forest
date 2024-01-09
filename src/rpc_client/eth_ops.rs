// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_api::data_types::MarketDeal;
use crate::{blocks::TipsetKeys, rpc_api::eth_api::*};

use super::{ApiInfo, RpcRequest};

impl ApiInfo {
    pub fn eth_accounts_req() -> RpcRequest<Vec<String>> {
        RpcRequest::new_v1(ETH_ACCOUNTS, ())
    }

    pub fn eth_block_number_req() -> RpcRequest<String> {
        RpcRequest::new_v1(ETH_BLOCK_NUMBER, ())
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

    pub fn state_market_storage_deal_req(deal_id: u64, tsk: TipsetKeys) -> RpcRequest<MarketDeal> {
        RpcRequest::new_v1(STATE_MARKET_STORAGE_DEAL, (deal_id, tsk))
    }
}
