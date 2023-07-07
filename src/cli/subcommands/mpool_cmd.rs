// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

use crate::blocks::Tipset;
use crate::json::cid::vec::CidJsonVec;
use crate::json::signed_message::json::SignedMessageJson;
use crate::message::{Message, SignedMessage};
use crate::rpc_client::{chain_ops::*, mpool_pending, state_ops::*, wallet_ops::*};
use crate::shim::address::StrictAddress;
use crate::shim::{address::Address, econ::TokenAmount};

use ahash::{HashMap, HashSet};
use clap::Subcommand;
use fvm_ipld_encoding::Cbor;
use num::BigInt;
use std::sync::Arc;

use super::{handle_rpc_err, Config};

#[derive(Debug, Subcommand)]
pub enum MpoolCommands {
    /// Get pending messages
    Pending {
        /// Print pending messages for addresses in local wallet only
        #[arg(long)]
        local: bool,
        /// Only print `CIDs` of messages in output
        #[arg(long)]
        cids: bool,
        /// Return messages to a given address
        #[arg(long)]
        to: Option<String>,
        /// Return messages from a given address
        #[arg(long)]
        from: Option<String>,
    },
    /// Print mempool stats
    Stat {
        /// Number of blocks to look back for minimum `basefee`
        #[arg(long, default_value = "60")]
        basefee_lookback: u32,
        /// Print stats for addresses in local wallet only
        #[arg(long)]
        local: bool,
    },
}

fn to_addr(value: &Option<String>) -> anyhow::Result<Option<StrictAddress>> {
    Ok(value
        .as_ref()
        .map(|s| StrictAddress::from_str(s))
        .transpose()?)
}

fn filter_messages(
    messages: Vec<SignedMessageJson>,
    local_addrs: Option<HashSet<Address>>,
    to: &Option<String>,
    from: &Option<String>,
) -> anyhow::Result<Vec<SignedMessageJson>> {
    let to = to_addr(to)?;
    let from = to_addr(from)?;

    let filtered = messages
        .into_iter()
        .filter(|msg| {
            local_addrs
                .as_ref()
                .map(|addrs| addrs.contains(&msg.0.from()))
                .unwrap_or(true)
                && to.map(|addr| msg.0.to() == addr.into()).unwrap_or(true)
                && from.map(|addr| msg.0.from() == addr.into()).unwrap_or(true)
        })
        .collect();

    Ok(filtered)
}

async fn get_actor_sequence(
    message: &SignedMessage,
    tipset: &Arc<Tipset>,
    config: &Config,
) -> Option<u64> {
    let address = message.from();
    let get_actor_result = state_get_actor(
        (address.to_owned().into(), tipset.key().to_owned().into()),
        &config.client.rpc_token,
    )
    .await;

    let actor_state = match get_actor_result {
        Ok(actor_json) => {
            if let Some(state) = actor_json {
                state.0
            } else {
                println!("{}, actor state not found", address);
                return None;
            }
        }
        Err(err) => {
            let error_message = match err {
                jsonrpc_v2::Error::Full { message, .. } => message,
                jsonrpc_v2::Error::Provided { message, .. } => message.to_string(),
            };

            println!("{}, err: {}", address, error_message);
            return None;
        }
    };

    Some(actor_state.sequence)
}

type StatBucket = HashMap<u64, SignedMessage>;

#[derive(Debug, Default)]
struct MpStat {
    address: String,
    past: u64,
    current: u64,
    future: u64,
    below_current: u64,
    below_past: u64,
    gas_limit: BigInt,
}

fn compute_stats(
    messages: &[SignedMessage],
    actor_sequences: HashMap<Address, u64>,
    curr_base_fee: TokenAmount,
    min_base_fee: TokenAmount,
) -> Vec<MpStat> {
    // for m in messages {
    //     println!("messages:\n{:?}", m);
    // }
    // println!("actor_sequences:\n{:?}", actor_sequences);
    // println!("curr_base_fee:\n{:?}", curr_base_fee);
    // println!("min_base_fee:\n{:?}", min_base_fee);

    let mut buckets = HashMap::<Address, StatBucket>::default();
    for msg in messages {
        buckets
            .entry(msg.from())
            .or_insert(StatBucket::default())
            .insert(msg.sequence(), msg.to_owned());
    }

    let mut stats: Vec<MpStat> = Vec::new();

    for (address, bucket) in buckets {
        let actor_sequence = actor_sequences[&address];

        let mut curr_sequence = actor_sequence;
        while bucket.get(&curr_sequence).is_some() {
            curr_sequence += 1;
        }

        let mut stat = MpStat {
            address: address.to_string(),
            ..Default::default()
        };

        for (_, msg) in bucket {
            if msg.sequence() < actor_sequence {
                stat.past += 1;
            } else if msg.sequence() > curr_sequence {
                stat.future += 1;
            } else {
                stat.current += 1;
            }

            if msg.gas_fee_cap() < curr_base_fee {
                stat.below_current += 1;
            }
            if msg.gas_fee_cap() < min_base_fee {
                stat.below_past += 1;
            }

            stat.gas_limit += msg.message().gas_limit;
        }

        stats.push(stat);
    }

    //println!("{:?}", stats);

    stats
}

fn print_stats(stats: &[MpStat], basefee_lookback: u32) {
    let mut total = MpStat::default();

    for stat in stats {
        total.past += stat.past;
        total.current += stat.current;
        total.future += stat.future;
        total.below_current += stat.below_current;
        total.below_past += stat.below_past;
        total.gas_limit += &stat.gas_limit;

        println!(
            "{}: Nonce past: {}, cur: {}, future: {}; FeeCap cur: {}, min-{}: {}, gasLimit: {}",
            stat.address,
            stat.past,
            stat.current,
            stat.future,
            stat.below_current,
            basefee_lookback,
            stat.below_past,
            stat.gas_limit
        );
    }

    println!("-----");
    println!(
        "total: Nonce past: {}, cur: {}, future: {}; FeeCap cur: {}, min-{}: {}, gasLimit: {}",
        total.past,
        total.current,
        total.future,
        total.below_current,
        basefee_lookback,
        total.below_past,
        total.gas_limit
    );
}

impl MpoolCommands {
    pub async fn run(self, config: Config) -> anyhow::Result<()> {
        match self {
            Self::Pending {
                local,
                cids,
                to,
                from,
            } => {
                let messages = mpool_pending((CidJsonVec(vec![]),), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;

                let local_addrs = if local {
                    let response = wallet_list((), &config.client.rpc_token)
                        .await
                        .map_err(handle_rpc_err)?;
                    Some(HashSet::from_iter(response.iter().map(|addr| addr.0)))
                } else {
                    None
                };

                let filtered_messages = filter_messages(messages, local_addrs, &to, &from)?;

                for msg in filtered_messages {
                    if cids {
                        println!("{}", msg.0.cid().unwrap());
                    } else {
                        println!("{}", serde_json::to_string_pretty(&msg)?);
                    }
                }

                Ok(())
            }
            Self::Stat {
                basefee_lookback,
                local,
            } => {
                let tipset = chain_head(&config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?
                    .0;

                let curr_base_fee = tipset.blocks()[0].parent_base_fee().to_owned();

                let atto_str =
                    chain_get_min_base_fee((basefee_lookback,), &config.client.rpc_token)
                        .await
                        .map_err(handle_rpc_err)?;
                let min_base_fee = TokenAmount::from_atto(atto_str.parse::<BigInt>()?);

                let messages = mpool_pending((CidJsonVec(vec![]),), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;

                let local_addrs = if local {
                    let response = wallet_list((), &config.client.rpc_token)
                        .await
                        .map_err(handle_rpc_err)?;
                    Some(HashSet::from_iter(response.iter().map(|addr| addr.0)))
                } else {
                    None
                };

                let signed_messages: Vec<SignedMessage> =
                    filter_messages(messages, local_addrs, &None, &None)?
                        .into_iter()
                        .map(|m| m.0)
                        .collect();

                let mut actor_sequences: HashMap<Address, u64> = HashMap::default();
                for msg in signed_messages.iter() {
                    if let Some(sequence) = get_actor_sequence(msg, &tipset, &config).await {
                        actor_sequences.insert(msg.from(), sequence);
                    }
                }

                let stats = compute_stats(
                    &signed_messages,
                    actor_sequences,
                    curr_base_fee,
                    min_base_fee,
                );

                print_stats(&stats, basefee_lookback);

                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_management::{KeyStore, KeyStoreConfig, Wallet};
    use crate::message::Message;
    use crate::message_pool::tests::create_smsg;
    use crate::shim::crypto::SignatureType;
    use std::borrow::BorrowMut;

    #[test]
    fn message_filtering_none() {
        let keystore = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new(keystore);
        let sender = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let target = wallet.generate_addr(SignatureType::Secp256k1).unwrap();

        let mut smsg_vec = Vec::new();
        for i in 0..4 {
            let msg = create_smsg(&target, &sender, wallet.borrow_mut(), i as u64, 1000000, 1);
            smsg_vec.push(msg);
        }

        let smsg_json_vec: Vec<SignedMessageJson> = smsg_vec
            .clone()
            .into_iter()
            .map(SignedMessageJson::from)
            .collect();

        // No filtering is set up
        let smsg_filtered: Vec<SignedMessage> = filter_messages(smsg_json_vec, None, &None, &None)
            .unwrap()
            .into_iter()
            .map(|m| m.0)
            .collect();

        assert_eq!(smsg_vec, smsg_filtered);
    }

    #[test]
    fn message_filtering_local() {
        let keystore = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new(keystore);
        let sender = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let target = wallet.generate_addr(SignatureType::Secp256k1).unwrap();

        let mut smsg_vec = Vec::new();
        for i in 0..4 {
            let msg = create_smsg(&target, &sender, wallet.borrow_mut(), i as u64, 1000000, 1);
            smsg_vec.push(msg);
        }

        // Create a message with adresses from an external wallet
        let ext_keystore = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut ext_wallet = Wallet::new(ext_keystore);
        let ext_sender = ext_wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let ext_target = ext_wallet.generate_addr(SignatureType::Secp256k1).unwrap();

        let msg = create_smsg(
            &ext_target,
            &ext_sender,
            ext_wallet.borrow_mut(),
            4,
            1000000,
            1,
        );
        smsg_vec.push(msg);

        let smsg_json_vec: Vec<SignedMessageJson> = smsg_vec
            .clone()
            .into_iter()
            .map(SignedMessageJson::from)
            .collect();
        let local_addrs = HashSet::from_iter(wallet.list_addrs().unwrap().into_iter());

        // Filter local addresses
        let smsg_filtered: Vec<SignedMessage> =
            filter_messages(smsg_json_vec, Some(local_addrs), &None, &None)
                .unwrap()
                .into_iter()
                .map(|m| m.0)
                .collect();

        for smsg in smsg_filtered.iter() {
            assert_eq!(smsg.from(), sender);
        }
    }

    #[test]
    fn message_filtering_from() {
        let keystore = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new(keystore);
        let sender = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let target = wallet.generate_addr(SignatureType::Secp256k1).unwrap();

        let mut smsg_vec = Vec::new();
        for i in 0..4 {
            let msg = create_smsg(&target, &sender, wallet.borrow_mut(), i as u64, 1000000, 1);
            smsg_vec.push(msg);
        }

        // Create a message from a second sender
        let sender2 = wallet.generate_addr(SignatureType::Secp256k1).unwrap();

        let msg = create_smsg(&target, &sender2, wallet.borrow_mut(), 4, 1000000, 1);
        smsg_vec.push(msg);

        let smsg_json_vec: Vec<SignedMessageJson> = smsg_vec
            .clone()
            .into_iter()
            .map(SignedMessageJson::from)
            .collect();

        // Filtering messages from sender2
        let smsg_filtered: Vec<SignedMessage> =
            filter_messages(smsg_json_vec, None, &None, &Some(sender2.to_string()))
                .unwrap()
                .into_iter()
                .map(|m| m.0)
                .collect();

        for smsg in smsg_filtered.iter() {
            assert_eq!(smsg.from(), sender2);
        }
    }

    #[test]
    fn message_filtering_to() {
        let keystore = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new(keystore);
        let sender = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let target = wallet.generate_addr(SignatureType::Secp256k1).unwrap();

        let mut smsg_vec = Vec::new();
        for i in 0..4 {
            let msg = create_smsg(&target, &sender, wallet.borrow_mut(), i as u64, 1000000, 1);
            smsg_vec.push(msg);
        }

        // Create a message to a second target
        let target2 = wallet.generate_addr(SignatureType::Secp256k1).unwrap();

        let msg = create_smsg(&target2, &sender, wallet.borrow_mut(), 4, 1000000, 1);
        smsg_vec.push(msg);

        let smsg_json_vec: Vec<SignedMessageJson> = smsg_vec
            .clone()
            .into_iter()
            .map(SignedMessageJson::from)
            .collect();

        // Filtering messages to target2
        let smsg_filtered: Vec<SignedMessage> =
            filter_messages(smsg_json_vec, None, &Some(target2.to_string()), &None)
                .unwrap()
                .into_iter()
                .map(|m| m.0)
                .collect();

        for smsg in smsg_filtered.iter() {
            assert_eq!(smsg.to(), target2);
        }
    }
}
