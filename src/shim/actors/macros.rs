// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

/// This macro iterates over each transaction, decodes the transaction key using variable-length integer encoding,
/// and constructs a Transaction object with the decoded data.
///
/// Parameters:
/// - `$res`: A mutable reference to a collection where the parsed transactions will be stored.
/// - `$txns`: A collection of transaction data to be parsed.
macro_rules! parse_pending_transactions {
    ($res:ident, $txns:expr) => {
        $txns.for_each(|tx_key, txn| {
            match integer_encoding::VarInt::decode_var(&tx_key) {
                Some((tx_id, _)) => {
                    $res.push(Transaction {
                        id: tx_id,
                        to: txn.to.into(),
                        value: txn.value.clone().into(),
                        method: txn.method,
                        params: txn.params.clone(),
                        approved: txn.approved.clone().into_iter().map(From::from).collect(),
                    });
                }
                None => anyhow::bail!("Error decoding varint"),
            }
            Ok(())
        })?;
    };
}

/// This macro iterates over each transaction, decodes the transaction key, and constructs a Transaction object.
///
/// Parameters:
/// - `$res`: A mutable reference to a collection where the parsed transactions will be stored.
/// - `$txns`: A collection of transaction data to be parsed.
macro_rules! parse_pending_transactions_v3 {
    ($res:ident, $txns:expr) => {
        $txns.for_each(|tx_key, txn| {
            match integer_encoding::VarInt::decode_var(&tx_key) {
                Some((tx_id, _)) => {
                    $res.push(Transaction {
                        id: tx_id,
                        to: txn.to.into(),
                        value: txn.value.clone().into(),
                        method: txn.method,
                        params: txn.params.clone(),
                        approved: txn.approved.clone().into_iter().map(From::from).collect(),
                    });
                }
                None => anyhow::bail!("Error decoding varint"),
            }
            Ok(())
        })?;
    };
}

/// This macro iterates over each transaction, assumes that transaction id's are directly available and not encoded.
///
/// Parameters:
/// - `$res`: A mutable reference to a collection where the parsed transactions will be stored.
/// - `$txns`: A collection of transaction data to be parsed.
macro_rules! parse_pending_transactions_v4 {
    ($res:ident, $txns:expr) => {
        $txns.for_each(|tx_id, txn| {
            $res.push(Transaction {
                id: tx_id.0,
                to: txn.to.into(),
                value: txn.value.clone().into(),
                method: txn.method,
                params: txn.params.clone(),
                approved: txn.approved.clone().into_iter().map(From::from).collect(),
            });
            Ok(())
        })?;
    };
}

pub(crate) use parse_pending_transactions;
pub(crate) use parse_pending_transactions_v3;
pub(crate) use parse_pending_transactions_v4;
