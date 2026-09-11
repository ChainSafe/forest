// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! The Ethereum view of a tipset's events.

use super::TipsetEvents;
use crate::blocks::Tipset;
use crate::eth::EthChainId;
use crate::message::ChainMessage;
use crate::rpc::eth::bloom::{Bloom, accrue_eth_log};
use crate::rpc::eth::types::{EthAddress, EthBytes, EthHash};
use crate::rpc::eth::{EVM_WORD_LENGTH, EthLog, EthUint64, eth_tx_hash_from_signed_message};
use crate::rpc::types::EventEntry;
use crate::state_manager::{ExecutedTipset, StateManager};
use fvm_ipld_encoding::IPLD_RAW;

/// The Ethereum logs of one tipset, grouped by message, in tipset order.
pub struct BlockLogs {
    by_message: Vec<Vec<EthLog>>,
}

impl BlockLogs {
    /// Collects the logs of an executed tipset.
    pub fn collect(
        state_manager: &StateManager,
        tipset: &Tipset,
        executed: &ExecutedTipset,
    ) -> anyhow::Result<Self> {
        Self::from_events(&TipsetEvents::collect(state_manager, tipset, executed))
    }

    fn from_events(events: &TipsetEvents) -> anyhow::Result<Self> {
        let block_hash: EthHash = events.tipset_key.cid()?.into();
        let block_number = EthUint64(events.height as u64);
        let mut by_message = vec![Vec::new(); events.message_count];
        for message in &events.messages {
            let Some(transaction_hash) = message.eth_tx_hash else {
                continue;
            };
            let logs = message
                .events
                .iter()
                .filter_map(|event| {
                    let address =
                        EthAddress::from_filecoin_address(&event.emitter_address?).ok()?;
                    let (data, topics) = eth_log_from_event(&event.entries)?;
                    Some(EthLog {
                        address,
                        data,
                        topics,
                        removed: false,
                        log_index: event.event_idx.into(),
                        transaction_index: message.msg_idx.into(),
                        transaction_hash,
                        block_hash,
                        block_number,
                    })
                })
                .collect();
            if let Some(slot) = by_message.get_mut(message.msg_idx as usize) {
                *slot = logs;
            }
        }
        Ok(Self { by_message })
    }

    /// All logs of the tipset, in tipset order.
    pub fn logs(&self) -> impl Iterator<Item = &EthLog> {
        self.by_message.iter().flatten()
    }

    /// The bloom of every log's address and topics.
    pub fn bloom(&self) -> Bloom {
        let mut bloom = Bloom::default();
        for log in self.logs() {
            accrue_eth_log(&mut bloom, &log.address, &log.topics);
        }
        bloom
    }
}

/// The Ethereum transaction hash of a message, when one can be derived from it.
pub(super) fn eth_tx_hash(message: &ChainMessage, eth_chain_id: EthChainId) -> Option<EthHash> {
    match message {
        ChainMessage::Signed(signed) => eth_tx_hash_from_signed_message(signed, eth_chain_id).ok(),
        ChainMessage::Unsigned(message) => Some(message.cid().into()),
    }
}

fn match_key(key: &str) -> Option<usize> {
    match key.get(0..2) {
        Some("t1") => Some(0),
        Some("t2") => Some(1),
        Some("t3") => Some(2),
        Some("t4") => Some(3),
        _ => None,
    }
}

/// The `(data, topics)` of an event in Ethereum form, or `None` when the event has none.
pub(crate) fn eth_log_from_event(entries: &[EventEntry]) -> Option<(EthBytes, Vec<EthHash>)> {
    let mut topics_found = [false; 4];
    let mut topics_found_count = 0;
    let mut data_found = false;
    let mut data: EthBytes = EthBytes::default();
    let mut topics: Vec<EthHash> = Vec::default();
    for entry in entries {
        // Drop events with non-raw topics. Built-in actors emit CBOR, and anything else would be
        // invalid anyway.
        if entry.codec != IPLD_RAW {
            return None;
        }
        // Check if the key is t1..t4
        if let Some(idx) = match_key(&entry.key) {
            // Drop events with mis-sized topics.
            let result: Result<[u8; EVM_WORD_LENGTH], _> = entry.value.0.as_slice().try_into();
            let bytes = if let Ok(value) = result {
                value
            } else {
                tracing::warn!(
                    "got an EVM event topic with an invalid size (key: {}, size: {})",
                    entry.key,
                    entry.value.0.len()
                );
                return None;
            };
            // Drop events with duplicate topics.
            if *topics_found.get(idx).expect("Infallible") {
                tracing::warn!("got a duplicate EVM event topic (key: {})", entry.key);
                return None;
            }
            *topics_found.get_mut(idx).expect("Infallible") = true;
            topics_found_count += 1;
            // Extend the topics array
            if topics.len() <= idx {
                topics.resize(idx + 1, EthHash::default());
            }
            *topics.get_mut(idx).expect("Infallible") = bytes.into();
        } else if entry.key == "d" {
            // Drop events with duplicate data fields.
            if data_found {
                tracing::warn!("got duplicate EVM event data");
                return None;
            }
            data_found = true;
            data = EthBytes(entry.value.0.clone());
        } else {
            // Skip entries we don't understand (makes it easier to extend things).
            // But we warn for now because we don't expect them.
            tracing::warn!("unexpected event entry (key: {})", entry.key);
        }
    }
    // Drop events with skipped topics.
    if topics.len() != topics_found_count {
        tracing::warn!(
            "EVM event topic length mismatch (expected: {}, actual: {})",
            topics.len(),
            topics_found_count
        );
        return None;
    }
    Some((data, topics))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::TipsetKey;
    use crate::rpc::eth::events::{MessageEvents, TipsetEvent};
    use crate::shim::address::Address;
    use crate::utils::multihash::MultihashCode;
    use cid::Cid;
    use fvm_ipld_encoding::DAG_CBOR;
    use fvm_shared4::event::Flags;
    use itertools::Itertools as _;
    use multihash_derive::MultihashDigest as _;
    use nunny::vec as nonempty;
    use std::str::FromStr as _;

    fn tipset_key(seed: u8) -> TipsetKey {
        TipsetKey::from(nonempty![Cid::new_v1(
            DAG_CBOR,
            MultihashCode::Identity.digest(&[seed])
        )])
    }

    /// An event has an Ethereum form only when its emitter resolves to an eth address, its
    /// message has an eth transaction hash and its entries are raw `t1..t4`/`d` (EMT-4, SHP-1,
    /// SHP-7); every log carries the tipset's identity and its message position (NUM-5).
    #[test]
    fn from_events_keeps_only_events_with_an_eth_form() {
        let contract = Address::from_str("t410f744ma4xsq3r3eczzktfj7goal67myzfkusna2hy").unwrap();
        let topic = EventEntry {
            flags: Flags::FLAG_INDEXED_ALL.bits(),
            key: "t1".into(),
            codec: IPLD_RAW,
            value: vec![7; EVM_WORD_LENGTH].into(),
        };
        let cbor_topic = EventEntry {
            codec: DAG_CBOR,
            ..topic.clone()
        };
        let tx_hash = EthHash(ethereum_types::H256::from_slice(&[1; EVM_WORD_LENGTH]));
        let event = |event_idx, emitter_address, entries| TipsetEvent {
            event_idx,
            emitter_address,
            entries,
        };
        let events = TipsetEvents {
            tipset_key: tipset_key(1),
            height: 5,
            message_count: 3,
            messages: vec![
                // Message 0: an EVM log, then an unresolvable emitter, then a built-in actor's
                // CBOR event.
                MessageEvents {
                    msg_idx: 0,
                    eth_tx_hash: Some(tx_hash),
                    events: vec![
                        event(0, Some(contract), vec![topic.clone()]),
                        event(1, None, vec![topic.clone()]),
                        event(2, Some(contract), vec![cbor_topic]),
                    ],
                },
                // Message 2: an EVM log from a message without an eth transaction hash.
                MessageEvents {
                    msg_idx: 2,
                    eth_tx_hash: None,
                    events: vec![event(3, Some(contract), vec![topic])],
                },
            ],
        };

        let logs = BlockLogs::from_events(&events).unwrap();

        assert_eq!(
            logs.by_message.iter().map(Vec::len).collect_vec(),
            [1, 0, 0]
        );
        let log = logs.logs().exactly_one().ok().unwrap();
        assert_eq!(
            log.address,
            EthAddress::from_filecoin_address(&contract).unwrap()
        );
        assert_eq!(
            log.topics,
            vec![EthHash(ethereum_types::H256([7; EVM_WORD_LENGTH]))]
        );
        assert_eq!(log.log_index, EthUint64(0));
        assert_eq!(log.transaction_index, EthUint64(0));
        assert_eq!(log.transaction_hash, tx_hash);
        assert_eq!(log.block_hash, events.tipset_key.cid().unwrap().into());
        assert_eq!(log.block_number, EthUint64(5));
        assert!(!log.removed);

        // The bloom is the fold of exactly those logs (BLM-2).
        let mut expected = Bloom::default();
        accrue_eth_log(&mut expected, &log.address, &log.topics);
        assert_eq!(logs.bloom(), expected);
    }

    #[test]
    fn test_eth_log_from_event() {
        // The value member of these event entries correspond to existing topics on Calibnet,
        // but they could just as easily be vectors filled with random bytes.

        let entries = vec![
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t1".into(),
                codec: IPLD_RAW,
                value: vec![
                    226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11,
                    81, 29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t2".into(),
                codec: IPLD_RAW,
                value: vec![
                    116, 4, 227, 209, 4, 234, 120, 65, 195, 217, 230, 253, 32, 173, 254, 153, 180,
                    173, 88, 107, 192, 141, 143, 59, 211, 175, 239, 137, 76, 241, 132, 222,
                ]
                .into(),
            },
        ];
        let (bytes, hashes) = eth_log_from_event(&entries).unwrap();
        assert!(bytes.0.is_empty());
        assert_eq!(hashes.len(), 2);

        let entries = vec![
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t1".into(),
                codec: IPLD_RAW,
                value: vec![
                    226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11,
                    81, 29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t2".into(),
                codec: IPLD_RAW,
                value: vec![
                    116, 4, 227, 209, 4, 234, 120, 65, 195, 217, 230, 253, 32, 173, 254, 153, 180,
                    173, 88, 107, 192, 141, 143, 59, 211, 175, 239, 137, 76, 241, 132, 222,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t3".into(),
                codec: IPLD_RAW,
                value: vec![
                    226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11,
                    81, 29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t4".into(),
                codec: IPLD_RAW,
                value: vec![
                    116, 4, 227, 209, 4, 234, 120, 65, 195, 217, 230, 253, 32, 173, 254, 153, 180,
                    173, 88, 107, 192, 141, 143, 59, 211, 175, 239, 137, 76, 241, 132, 222,
                ]
                .into(),
            },
        ];
        let (bytes, hashes) = eth_log_from_event(&entries).unwrap();
        assert!(bytes.0.is_empty());
        assert_eq!(hashes.len(), 4);

        let entries = vec![
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t1".into(),
                codec: IPLD_RAW,
                value: vec![
                    226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11,
                    81, 29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t1".into(),
                codec: IPLD_RAW,
                value: vec![
                    116, 4, 227, 209, 4, 234, 120, 65, 195, 217, 230, 253, 32, 173, 254, 153, 180,
                    173, 88, 107, 192, 141, 143, 59, 211, 175, 239, 137, 76, 241, 132, 222,
                ]
                .into(),
            },
        ];
        assert!(eth_log_from_event(&entries).is_none());

        let entries = vec![
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t3".into(),
                codec: IPLD_RAW,
                value: vec![
                    226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11,
                    81, 29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t4".into(),
                codec: IPLD_RAW,
                value: vec![
                    116, 4, 227, 209, 4, 234, 120, 65, 195, 217, 230, 253, 32, 173, 254, 153, 180,
                    173, 88, 107, 192, 141, 143, 59, 211, 175, 239, 137, 76, 241, 132, 222,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t1".into(),
                codec: IPLD_RAW,
                value: vec![
                    226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11,
                    81, 29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t2".into(),
                codec: IPLD_RAW,
                value: vec![
                    116, 4, 227, 209, 4, 234, 120, 65, 195, 217, 230, 253, 32, 173, 254, 153, 180,
                    173, 88, 107, 192, 141, 143, 59, 211, 175, 239, 137, 76, 241, 132, 222,
                ]
                .into(),
            },
        ];
        let (bytes, hashes) = eth_log_from_event(&entries).unwrap();
        assert!(bytes.0.is_empty());
        assert_eq!(hashes.len(), 4);

        let entries = vec![
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t1".into(),
                codec: IPLD_RAW,
                value: vec![
                    226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11,
                    81, 29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t3".into(),
                codec: IPLD_RAW,
                value: vec![
                    116, 4, 227, 209, 4, 234, 120, 65, 195, 217, 230, 253, 32, 173, 254, 153, 180,
                    173, 88, 107, 192, 141, 143, 59, 211, 175, 239, 137, 76, 241, 132, 222,
                ]
                .into(),
            },
        ];
        assert!(eth_log_from_event(&entries).is_none());

        let entries = vec![EventEntry {
            flags: (Flags::FLAG_INDEXED_ALL).bits(),
            key: "t1".into(),
            codec: DAG_CBOR,
            value: vec![
                226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11, 81,
                29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149,
            ]
            .into(),
        }];
        assert!(eth_log_from_event(&entries).is_none());

        let entries = vec![EventEntry {
            flags: (Flags::FLAG_INDEXED_ALL).bits(),
            key: "t1".into(),
            codec: IPLD_RAW,
            value: vec![
                226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11, 81,
                29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149, 0,
            ]
            .into(),
        }];
        assert!(eth_log_from_event(&entries).is_none());

        let entries = vec![
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t1".into(),
                codec: IPLD_RAW,
                value: vec![
                    226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11,
                    81, 29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "d".into(),
                codec: IPLD_RAW,
                value: vec![
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 49, 190,
                    25, 34, 116, 232, 27, 26, 248,
                ]
                .into(),
            },
        ];
        let (bytes, hashes) = eth_log_from_event(&entries).unwrap();
        assert_eq!(bytes.0.len(), 32);
        assert_eq!(hashes.len(), 1);

        let entries = vec![
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "t1".into(),
                codec: IPLD_RAW,
                value: vec![
                    226, 71, 32, 244, 92, 183, 79, 45, 85, 241, 222, 235, 182, 9, 143, 80, 241, 11,
                    81, 29, 171, 138, 125, 71, 196, 129, 154, 8, 220, 208, 184, 149, 0,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "d".into(),
                codec: IPLD_RAW,
                value: vec![
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 49, 190,
                    25, 34, 116, 232, 27, 26, 248,
                ]
                .into(),
            },
            EventEntry {
                flags: (Flags::FLAG_INDEXED_ALL).bits(),
                key: "d".into(),
                codec: IPLD_RAW,
                value: vec![
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 49, 190,
                    25, 34, 116, 232, 27, 26, 248,
                ]
                .into(),
            },
        ];
        assert!(eth_log_from_event(&entries).is_none());
    }
}
