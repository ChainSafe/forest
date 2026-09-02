// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! The Ethereum view of a tipset's events.

use crate::rpc::eth::EVM_WORD_LENGTH;
use crate::rpc::eth::types::{EthBytes, EthHash};
use crate::rpc::types::EventEntry;
use fvm_ipld_encoding::IPLD_RAW;

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
    use fvm_ipld_encoding::DAG_CBOR;
    use fvm_shared4::event::Flags;

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
