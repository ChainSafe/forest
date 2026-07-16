// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Ethereum block logs bloom support: the [`Bloom`] type and the derivation and storage of
//! per-tipset block logs blooms.
//!
//! Blooms are stored when a tipset is executed, alongside its receipts and events, and
//! served from the store when building Ethereum blocks. For tipsets never executed by
//! this node (nor covered by index backfill), the block reports [`FULL_BLOOM`].

use super::*;
use crate::db::EthBlockBloomStore;

/// Ethereum Bloom filter size in bits.
/// Bloom filter is used in Ethereum to minimize the number of block queries.
const BLOOM_SIZE: usize = 2048;

/// Ethereum Bloom filter size in bytes.
const BLOOM_SIZE_IN_BYTES: usize = BLOOM_SIZE / 8;

/// Ethereum Bloom filter with all bits set to 1.
pub(super) const FULL_BLOOM: [u8; BLOOM_SIZE_IN_BYTES] = [0xff; BLOOM_SIZE_IN_BYTES];

/// Ethereum Bloom filter with all bits set to 0.
pub(super) const EMPTY_BLOOM: [u8; BLOOM_SIZE_IN_BYTES] = [0x0; BLOOM_SIZE_IN_BYTES];

/// Environment variable that enables computing (and storing) a block's logs bloom on a read
/// miss, instead of reporting [`FULL_BLOOM`].
pub(crate) const COMPUTE_BLOOM_ON_MISS_ENV: &str = "FOREST_ETH_RPC_COMPUTE_BLOOM_ON_MISS";

#[derive(PartialEq, Debug, Deserialize, Serialize, Default, Clone, JsonSchema, GetSize)]
pub struct Bloom(
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json::hexify_bytes")]
    #[get_size(ignore)]
    pub ethereum_types::Bloom,
);
lotus_json_with_self!(Bloom);

impl Bloom {
    /// Accrues the raw input bytes into the bloom filter.
    pub fn accrue(&mut self, input: &[u8]) {
        self.0.accrue(ethereum_types::BloomInput::Raw(input));
    }
}

/// Accrues an Ethereum log (its emitter address and topics) into the given bloom.
pub(super) fn accrue_eth_log(bloom: &mut Bloom, address: &EthAddress, topics: &[EthHash]) {
    for topic in topics {
        bloom.accrue(topic.0.as_bytes());
    }
    bloom.accrue(address.0.as_bytes());
}

/// Computes the block logs bloom of a tipset directly from its executed messages, resolving
/// event emitters against the post-execution state root.
fn compute_block_logs_bloom(
    state_manager: &StateManager,
    state_root: &Cid,
    executed_messages: &[ExecutedMessage],
) -> anyhow::Result<Bloom> {
    let state_tree = state_manager.get_state_tree(state_root)?;
    let mut resolved_eth_addrs = HashMap::default();
    let mut bloom = Bloom::default();
    for executed_message in executed_messages {
        let Some(events) = &executed_message.events else {
            continue;
        };
        for event in events {
            let emitter = event.emitter();
            let address = resolved_eth_addrs.entry(emitter).or_insert_with(|| {
                state_tree
                    .resolve_to_deterministic_address(
                        state_manager.chain_store().db(),
                        FilecoinAddress::new_id(emitter),
                    )
                    .ok()
                    .and_then(|addr| EthAddress::from_filecoin_address(&addr).ok())
            });
            let Some(address) = address else {
                continue;
            };
            let entries: Vec<EventEntry> = event
                .entries()
                .into_iter()
                .map(|entry| {
                    let (flags, key, codec, value) = entry.into_parts();
                    EventEntry {
                        flags,
                        key,
                        codec,
                        value: value.into(),
                    }
                })
                .collect();
            let Some((_data, topics)) = eth_log_from_event(&entries) else {
                continue;
            };
            accrue_eth_log(&mut bloom, address, &topics);
        }
    }
    Ok(bloom)
}

/// Computes and stores the block logs bloom of an executed tipset so that serving it later
/// is a plain read. Called when a tipset is executed and from index backfill.
pub(crate) fn store_block_logs_bloom(
    state_manager: &StateManager,
    tipset: &Tipset,
    state_root: &Cid,
    executed_messages: &[ExecutedMessage],
) -> anyhow::Result<()> {
    let key = tipset.key().cid()?;
    if state_manager.db().read_bloom(&key)?.is_some() {
        return Ok(());
    }
    let bloom = compute_block_logs_bloom(state_manager, state_root, executed_messages)?;
    state_manager
        .db()
        .write_bloom(&key, tipset.epoch(), &bloom.0.0)
}

/// Returns the block's logs bloom: the stored bloom when available, otherwise a full
/// (all-ones) bloom.
/// Setting [`COMPUTE_BLOOM_ON_MISS_ENV`] computes and stores the bloom on a miss instead.
pub(super) fn block_logs_bloom(
    state_manager: &StateManager,
    tipset: &Tipset,
    state_root: &Cid,
    executed_messages: &[ExecutedMessage],
) -> anyhow::Result<Bloom> {
    crate::def_is_env_truthy!(compute_bloom_on_miss, COMPUTE_BLOOM_ON_MISS_ENV);

    let key = tipset.key().cid()?;
    if let Some(bloom) = state_manager.db().read_bloom(&key)? {
        return Ok(Bloom(ethereum_types::Bloom(bloom)));
    }

    if compute_bloom_on_miss() {
        let bloom = compute_block_logs_bloom(state_manager, state_root, executed_messages)?;
        state_manager
            .db()
            .write_bloom(&key, tipset.epoch(), &bloom.0.0)?;
        return Ok(bloom);
    }
    Ok(Bloom(ethereum_types::Bloom(FULL_BLOOM)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accrue_eth_log_and_block_bloom_decomposition() {
        let empty = Bloom::default();
        let full = Bloom(ethereum_types::Bloom(FULL_BLOOM));

        // No logs yields the all-zeros bloom — the "definitely no events here" case
        // indexers rely on.
        assert_eq!(empty.0.0, EMPTY_BLOOM);

        let addr_a = EthAddress(ethereum_types::H160::from_slice(&[0x11; ADDRESS_LENGTH]));
        let topic_a = EthHash(ethereum_types::H256::from_slice(&[0x22; EVM_WORD_LENGTH]));
        let addr_b = EthAddress(ethereum_types::H160::from_slice(&[0x33; ADDRESS_LENGTH]));
        let topic_b = EthHash(ethereum_types::H256::from_slice(&[0x44; EVM_WORD_LENGTH]));

        // A real log sets some bits, but not all of them.
        let mut bloom_a = empty.clone();
        accrue_eth_log(&mut bloom_a, &addr_a, std::slice::from_ref(&topic_a));
        assert_ne!(bloom_a, empty);
        assert_ne!(bloom_a, full);

        let mut bloom_b = empty.clone();
        accrue_eth_log(&mut bloom_b, &addr_b, std::slice::from_ref(&topic_b));

        // The block bloom (both logs) equals the bitwise OR of the two individual
        // (receipt) blooms.
        let mut combined = bloom_a.clone();
        accrue_eth_log(&mut combined, &addr_b, std::slice::from_ref(&topic_b));

        let mut expected = bloom_a.0.0;
        for (out, b) in expected.iter_mut().zip(bloom_b.0.0.iter()) {
            *out |= *b;
        }
        assert_eq!(combined.0.0, expected);

        // Accruing the same log twice equals accruing it once.
        let mut twice = bloom_a.clone();
        accrue_eth_log(&mut twice, &addr_a, std::slice::from_ref(&topic_a));
        assert_eq!(twice, bloom_a);
    }
}
