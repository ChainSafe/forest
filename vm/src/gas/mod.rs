// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod price_list;

pub use self::price_list::PriceList;
use clock::ChainEpoch;

const BASE_PRICES: PriceList = PriceList {
    on_chain_message_base: 0,
    on_chain_message_per_byte: 2,
    on_chain_return_value_per_byte: 8,
    send_base: 5,
    send_transfer_funds: 5,
    send_invoke_method: 10,
    ipld_get_base: 10,
    ipld_get_per_byte: 1,
    ipld_put_base: 20,
    ipld_put_per_byte: 2,
    create_actor_base: 40,
    create_actor_extra: 500,
    delete_actor: -500,
    hashing_base: 5,
    hashing_per_byte: 2,
    compute_unsealed_sector_cid_base: 100,
    verify_seal_base: 2000,
    verify_post_base: 700,
    verify_consensus_fault: 10,
};

/// Returns gas price list by Epoch for gas consumption
pub fn price_list_by_epoch(_epoch: ChainEpoch) -> PriceList {
    // In future will match on epoch and select matching price lists when config options allowed
    BASE_PRICES
}
