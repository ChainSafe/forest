// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

/// This macro iterates over each transaction, decodes the transaction key using variable-length integer encoding,
/// and constructs a Transaction object with the decoded data.
///
/// Parameters:
/// - `$res`: A mutable reference to a collection where the parsed transactions will be stored.
/// - `$txns`: A collection of transaction data to be parsed.
#[macro_export]
macro_rules! parse_pending_transactions {
    ($res:ident, $txns:expr) => {
        $txns.for_each(|tx_key, txn| {
            match integer_encoding::VarInt::decode_var(&tx_key) {
                Some((tx_id, _)) => {
                    $res.push(Transaction {
                        id: tx_id,
                        to: txn.to,
                        value: txn.value.clone(),
                        method: txn.method,
                        params: txn.params.clone(),
                        approved: txn.approved.clone(),
                    });
                }
                None => anyhow::bail!("Error decoding varint"),
            }
            Ok(())
        })?;
    };
}

/// This macro iterates over each transaction, decodes the transaction key, and constructs a Transaction object
/// with additional processing for address and token formats using `from_address_v3_to_v2` and `from_token_v3_to_v2`.
///
/// Parameters:
/// - `$res`: A mutable reference to a collection where the parsed transactions will be stored.
/// - `$txns`: A collection of transaction data to be parsed.
#[macro_export]
macro_rules! parse_pending_transactions_v3 {
    ($res:ident, $txns:expr) => {
        $txns.for_each(|tx_key, txn| {
            match integer_encoding::VarInt::decode_var(&tx_key) {
                Some((tx_id, _)) => {
                    $res.push(Transaction {
                        id: tx_id,
                        to: from_address_v3_to_v2(txn.to),
                        value: from_token_v3_to_v2(&txn.value),
                        method: txn.method,
                        params: txn.params.clone(),
                        approved: txn
                            .approved
                            .iter()
                            .map(|&addr| from_address_v3_to_v2(addr))
                            .collect(),
                    });
                }
                None => anyhow::bail!("Error decoding varint"),
            }
            Ok(())
        })?;
    };
}

/// This macro iterates over each transaction, assumes that transaction id's are directly available and not encoded.
/// It constructs Transaction objects with transformations for address and token data from version 4 to version 2
/// using `from_address_v4_to_v2` and `from_token_v4_to_v2`.
///
/// Parameters:
/// - `$res`: A mutable reference to a collection where the parsed transactions will be stored.
/// - `$txns`: A collection of transaction data to be parsed.
#[macro_export]
macro_rules! parse_pending_transactions_v4 {
    ($res:ident, $txns:expr) => {
        $txns.for_each(|tx_id, txn| {
            $res.push(Transaction {
                id: tx_id.0,
                to: from_address_v4_to_v2(txn.to),
                value: from_token_v4_to_v2(&txn.value),
                method: txn.method,
                params: txn.params.clone(),
                approved: txn
                    .approved
                    .iter()
                    .map(|&addr| from_address_v4_to_v2(addr))
                    .collect(),
            });
            Ok(())
        })?;
    };
}
