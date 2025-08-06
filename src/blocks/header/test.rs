// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use cid::Cid;
use std::sync::LazyLock;

// See <https://github.com/filecoin-project/lotus/blob/d3ca54d617f4783a1a492993f06e737ea87a5834/chain/gen/genesis/genesis.go#L627>
// and <https://github.com/filecoin-project/lotus/commit/13e5b72cdbbe4a02f3863c04f9ecb69c21c3f80f#diff-fda2789d966ea533e74741c076f163070cbc7eb265b5513cd0c0f3bdee87245cR437>
pub static FILECOIN_GENESIS_CID: LazyLock<Cid> = LazyLock::new(|| {
    "bafyreiaqpwbbyjo4a42saasj36kkrpv4tsherf2e7bvezkert2a7dhonoi"
        .parse()
        .expect("Infallible")
});

pub static FILECOIN_GENESIS_BLOCK: LazyLock<Vec<u8>> = LazyLock::new(|| {
    hex::decode("a5684461746574696d6573323031372d30352d30352030313a32373a3531674e6574776f726b6846696c65636f696e65546f6b656e6846696c65636f696e6c546f6b656e416d6f756e7473a36b546f74616c537570706c796d322c3030302c3030302c303030664d696e6572736d312c3430302c3030302c3030306c50726f746f636f6c4c616273a36b446576656c6f706d656e746b3330302c3030302c3030306b46756e6472616973696e676b3230302c3030302c3030306a466f756e646174696f6e6b3130302c3030302c303030674d657373616765784854686973206973207468652047656e6573697320426c6f636b206f66207468652046696c65636f696e20446563656e7472616c697a65642053746f72616765204e6574776f726b2e")
        .expect("Infallible")
});

pub static GENESIS_BLOCK_PARENTS: LazyLock<TipsetKey> =
    LazyLock::new(|| nunny::vec![*FILECOIN_GENESIS_CID].into());

impl Default for RawBlockHeader {
    fn default() -> Self {
        Self {
            parents: GENESIS_BLOCK_PARENTS.clone(),
            miner_address: Default::default(),
            ticket: Default::default(),
            election_proof: Default::default(),
            beacon_entries: Default::default(),
            winning_post_proof: Default::default(),
            weight: Default::default(),
            epoch: Default::default(),
            state_root: Default::default(),
            message_receipts: Default::default(),
            messages: Default::default(),
            bls_aggregate: Default::default(),
            timestamp: Default::default(),
            signature: Default::default(),
            fork_signal: Default::default(),
            parent_base_fee: Default::default(),
        }
    }
}
