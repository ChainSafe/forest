// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::Tipset;
use crate::lotus_json::{HasLotusJson as _, NotNullVec};
use crate::message::{MessageRead as _, SignedMessage};
use crate::rpc::{self, prelude::*, types::ApiTipsetKey};
use crate::shim::address::StrictAddress;
use crate::shim::message::{METHOD_SEND, Message};
use crate::shim::{address::Address, econ::TokenAmount};

use ahash::{HashMap, HashSet};
use anyhow::Context as _;
use clap::Subcommand;
use fvm_ipld_encoding::RawBytes;
use num::BigInt;
use std::ops::Range;

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
        to: Option<StrictAddress>,
        /// Return messages from a given address
        #[arg(long)]
        from: Option<StrictAddress>,
    },
    /// Get the current nonce for an address
    Nonce {
        /// Address to check nonce for
        address: StrictAddress,
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
    /// Fill an on-chain nonce gap by pushing signed self-transfer messages.
    NonceFix {
        /// Address to fill nonces for (must be signable by the node's wallet).
        #[arg(long)]
        addr: StrictAddress,
        /// Derive the fill range from chain state and the mempool (ignores `--start` / `--end`).
        #[arg(long)]
        auto: bool,
        /// First sequence to fill (inclusive); required unless `--auto`.
        #[arg(long)]
        start: Option<u64>,
        /// End of range (exclusive); required unless `--auto`.
        #[arg(long)]
        end: Option<u64>,
        /// Gas fee cap for filler messages, in `attoFIL`. Default: twice the parent base fee from chain head.
        #[arg(long)]
        gas_fee_cap: Option<String>,
    },
}

fn filter_messages(
    messages: Vec<SignedMessage>,
    local_addrs: Option<HashSet<Address>>,
    to: Option<&StrictAddress>,
    from: Option<&StrictAddress>,
) -> anyhow::Result<Vec<SignedMessage>> {
    let filtered = messages
        .into_iter()
        .filter(|msg| {
            local_addrs
                .as_ref()
                .map(|addrs| addrs.contains(&msg.from()))
                .unwrap_or(true)
                && to.map(|addr| msg.to() == (*addr).into()).unwrap_or(true)
                && from
                    .map(|addr| msg.from() == (*addr).into())
                    .unwrap_or(true)
        })
        .collect();

    Ok(filtered)
}

enum NonceFixFillRangeInput {
    Auto {
        addr: Address,
        next_on_chain_nonce: u64,
        pending: Vec<SignedMessage>,
    },
    Manual {
        start: Option<u64>,
        end: Option<u64>,
    },
}

fn get_nonce_fix_fill_range(input: NonceFixFillRangeInput) -> anyhow::Result<Option<Range<u64>>> {
    match input {
        NonceFixFillRangeInput::Auto {
            addr,
            next_on_chain_nonce,
            pending,
        } => {
            let Some(pending_nonce) = pending
                .iter()
                .filter(|m| m.from() == addr)
                .map(|m| m.sequence())
                .filter(|&seq| seq >= next_on_chain_nonce)
                .min()
            else {
                return Ok(None);
            };
            if pending_nonce == next_on_chain_nonce {
                return Ok(None);
            }
            Ok(Some(next_on_chain_nonce..pending_nonce))
        }
        NonceFixFillRangeInput::Manual { start, end } => {
            let start = start.context("manual mode requires --start")?;
            let end = end.context("manual mode requires --end")?;
            anyhow::ensure!(end > start, "--end must be greater than --start");
            Ok(Some(start..end))
        }
    }
}

fn get_nonce_fix_gas_fee_cap(
    gas_fee_cap: Option<&str>,
    parent_base_fee: TokenAmount,
) -> anyhow::Result<TokenAmount> {
    if let Some(cap) = gas_fee_cap {
        Ok(TokenAmount::from_atto(
            cap.parse::<BigInt>()
                .context("invalid --gas-fee-cap value")?,
        ))
    } else {
        Ok(parent_base_fee * 2u64)
    }
}

async fn get_actor_sequence(
    message: &Message,
    tipset: &Tipset,
    client: &rpc::Client,
) -> Option<u64> {
    let address = message.from;
    let get_actor_result = StateGetActor::call(client, (address, tipset.key().into())).await;
    let actor_state = match get_actor_result {
        Ok(maybe_actor) => {
            if let Some(state) = maybe_actor {
                state
            } else {
                println!("{address}, actor state not found");
                return None;
            }
        }
        Err(err) => {
            println!("{address}, err: {err}");
            return None;
        }
    };

    Some(actor_state.sequence)
}

type StatBucket = HashMap<u64, Message>;

#[derive(Debug, Default, Eq, PartialEq)]
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
    messages: &[Message],
    actor_sequences: HashMap<Address, u64>,
    curr_base_fee: TokenAmount,
    min_base_fee: TokenAmount,
) -> Vec<MpStat> {
    let mut buckets = HashMap::<Address, StatBucket>::default();
    for msg in messages {
        buckets
            .entry(msg.from)
            .or_insert(StatBucket::default())
            .insert(msg.sequence, msg.to_owned());
    }

    let mut stats: Vec<MpStat> = Vec::with_capacity(buckets.len());

    for (address, bucket) in buckets {
        let actor_sequence = *actor_sequences.get(&address).expect("get must succeed");

        let mut curr_sequence = actor_sequence;
        while bucket.contains_key(&curr_sequence) {
            curr_sequence += 1;
        }

        let mut stat = MpStat {
            address: address.to_string(),
            ..Default::default()
        };

        for (_, msg) in bucket {
            if msg.sequence < actor_sequence {
                stat.past += 1;
            } else if msg.sequence > curr_sequence {
                stat.future += 1;
            } else {
                stat.current += 1;
            }

            if msg.gas_fee_cap < curr_base_fee {
                stat.below_current += 1;
            }
            if msg.gas_fee_cap < min_base_fee {
                stat.below_past += 1;
            }

            stat.gas_limit += msg.gas_limit;
        }

        stats.push(stat);
    }

    stats.sort_by(|m1, m2| m1.address.cmp(&m2.address));
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
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        match self {
            Self::Pending {
                local,
                cids,
                to,
                from,
            } => {
                let NotNullVec(messages) =
                    MpoolPending::call(&client, (ApiTipsetKey(None),)).await?;

                let local_addrs = if local {
                    let response = WalletList::call(&client, ()).await?;
                    Some(HashSet::from_iter(response))
                } else {
                    None
                };

                let filtered_messages =
                    filter_messages(messages, local_addrs, to.as_ref(), from.as_ref())?;

                for msg in filtered_messages {
                    if cids {
                        println!("{}", msg.cid());
                    } else {
                        println!("{}", msg.into_lotus_json_string_pretty()?);
                    }
                }

                Ok(())
            }
            Self::Stat {
                basefee_lookback,
                local,
            } => {
                let tipset = ChainHead::call(&client, ()).await?;
                let curr_base_fee = tipset.block_headers().first().parent_base_fee.to_owned();

                let (atto_str, NotNullVec(messages)) = tokio::try_join!(
                    ChainGetMinBaseFee::call(&client, (basefee_lookback,)),
                    MpoolPending::call(&client, (ApiTipsetKey(None),)),
                )?;
                let min_base_fee = TokenAmount::from_atto(atto_str.parse::<BigInt>()?);

                let local_addrs = if local {
                    let response = WalletList::call(&client, ()).await?;
                    Some(HashSet::from_iter(response))
                } else {
                    None
                };

                let messages: Vec<Message> = filter_messages(messages, local_addrs, None, None)?
                    .into_iter()
                    .map(|it| it.message)
                    .collect();

                let mut actor_sequences: HashMap<Address, u64> = HashMap::default();
                for msg in messages.iter() {
                    if let Some(sequence) = get_actor_sequence(msg, &tipset, &client).await {
                        actor_sequences.insert(msg.from, sequence);
                    }
                }

                let stats = compute_stats(&messages, actor_sequences, curr_base_fee, min_base_fee);

                print_stats(&stats, basefee_lookback);

                Ok(())
            }
            Self::Nonce { address } => {
                let nonce = MpoolGetNonce::call(&client, (address.into(),)).await?;
                println!("{nonce}");

                Ok(())
            }
            Self::NonceFix {
                addr,
                auto,
                start,
                end,
                gas_fee_cap,
            } => {
                let addr: Address = addr.into();

                let fill_range = if auto {
                    let actor = StateGetActor::call(&client, (addr, ApiTipsetKey(None)))
                        .await?
                        .with_context(|| format!("no on-chain actor found for {addr}"))?;
                    let next_nonce = actor.sequence;
                    let NotNullVec(pending) =
                        MpoolPending::call(&client, (ApiTipsetKey(None),)).await?;
                    get_nonce_fix_fill_range(NonceFixFillRangeInput::Auto {
                        addr,
                        next_on_chain_nonce: next_nonce,
                        pending,
                    })?
                } else {
                    get_nonce_fix_fill_range(NonceFixFillRangeInput::Manual { start, end })?
                };

                let Some(fill_range) = fill_range else {
                    println!("No nonce gap found or no --end flag specified");
                    return Ok(());
                };

                let tipset = ChainHead::call(&client, ()).await?;
                let parent_base_fee = tipset.block_headers().first().parent_base_fee.clone();
                let fee_cap = get_nonce_fix_gas_fee_cap(gas_fee_cap.as_deref(), parent_base_fee)?;
                let n = fill_range.end.saturating_sub(fill_range.start);
                println!(
                    "Creating {n} filler messages ({} ~ {})",
                    fill_range.start, fill_range.end
                );

                for sequence in fill_range {
                    let msg = Message {
                        version: 0,
                        from: addr,
                        to: addr,
                        sequence,
                        value: TokenAmount::default(),
                        method_num: METHOD_SEND,
                        params: RawBytes::new(vec![]),
                        gas_limit: 1_000_000,
                        gas_fee_cap: fee_cap.clone(),
                        gas_premium: TokenAmount::from_atto(5u64),
                    };
                    let smsg = WalletSignMessage::call(&client, (addr, msg)).await?;
                    MpoolPush::call(&client, (smsg,)).await?;
                }

                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_management::{KeyStore, KeyStoreConfig, Wallet};
    use crate::message_pool::tests::create_smsg;
    use crate::shim::crypto::SignatureType;
    use itertools::Itertools as _;
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

        let smsg_json_vec = smsg_vec.clone().into_iter().collect_vec();

        // No filtering is set up
        let smsg_filtered: Vec<SignedMessage> = filter_messages(smsg_json_vec, None, None, None)
            .unwrap()
            .into_iter()
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

        // Create a message with addresses from an external wallet
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

        let smsg_json_vec: Vec<SignedMessage> = smsg_vec.clone().into_iter().collect();
        let local_addrs = HashSet::from_iter(wallet.list_addrs().unwrap());

        // Filter local addresses
        let smsg_filtered: Vec<SignedMessage> =
            filter_messages(smsg_json_vec, Some(local_addrs), None, None)
                .unwrap()
                .into_iter()
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

        let smsg_json_vec: Vec<SignedMessage> = smsg_vec.clone().into_iter().collect();

        // Filtering messages from sender2
        let smsg_filtered: Vec<SignedMessage> =
            filter_messages(smsg_json_vec, None, None, Some(&sender2.into()))
                .unwrap()
                .into_iter()
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

        let smsg_json_vec: Vec<SignedMessage> = smsg_vec.clone().into_iter().collect();

        // Filtering messages to target2
        let smsg_filtered: Vec<SignedMessage> =
            filter_messages(smsg_json_vec, None, Some(&target2.into()), None)
                .unwrap()
                .into_iter()
                .collect();

        for smsg in smsg_filtered.iter() {
            assert_eq!(smsg.to(), target2);
        }
    }

    #[test]
    fn nonce_fix_auto_no_pending() {
        let keystore = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new(keystore);
        let addr = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let r = get_nonce_fix_fill_range(NonceFixFillRangeInput::Auto {
            addr,
            next_on_chain_nonce: 0,
            pending: vec![],
        })
        .unwrap();
        assert_eq!(r, None);
    }

    #[test]
    fn nonce_fix_auto_other_sender() {
        let keystore = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new(keystore);
        let addr = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let other = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let target = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let m = create_smsg(&target, &other, wallet.borrow_mut(), 10, 1000000, 1);
        let r = get_nonce_fix_fill_range(NonceFixFillRangeInput::Auto {
            addr,
            next_on_chain_nonce: 5,
            pending: vec![m],
        })
        .unwrap();
        assert_eq!(r, None);
    }

    #[test]
    fn nonce_fix_auto_fill_range_gap() {
        let keystore = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new(keystore);
        let addr = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let target = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let m = create_smsg(&target, &addr, wallet.borrow_mut(), 7, 1000000, 1);
        let r = get_nonce_fix_fill_range(NonceFixFillRangeInput::Auto {
            addr,
            next_on_chain_nonce: 5,
            pending: vec![m],
        })
        .unwrap();
        assert_eq!(r, Some(5..7));
    }

    #[test]
    fn nonce_fix_auto_fill_range_min_pending_nonce() {
        let keystore = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new(keystore);
        let addr = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let target = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let m10 = create_smsg(&target, &addr, wallet.borrow_mut(), 10, 1000000, 1);
        let m8 = create_smsg(&target, &addr, wallet.borrow_mut(), 8, 1000000, 1);
        let r = get_nonce_fix_fill_range(NonceFixFillRangeInput::Auto {
            addr,
            next_on_chain_nonce: 5,
            pending: vec![m10, m8],
        })
        .unwrap();
        assert_eq!(r, Some(5..8));
    }

    #[test]
    fn nonce_fix_auto_next_nonce_exist_in_mpool() {
        let keystore = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new(keystore);
        let addr = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let target = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let m = create_smsg(&target, &addr, wallet.borrow_mut(), 5, 1000000, 1);
        let r = get_nonce_fix_fill_range(NonceFixFillRangeInput::Auto {
            addr,
            next_on_chain_nonce: 5,
            pending: vec![m],
        })
        .unwrap();
        assert_eq!(r, None);
    }

    #[test]
    fn nonce_fix_manual_fill_range_missing_start() {
        let e = get_nonce_fix_fill_range(NonceFixFillRangeInput::Manual {
            start: None,
            end: Some(10),
        })
        .unwrap_err();
        assert!(
            e.to_string().contains("manual mode requires --start"),
            "{e}"
        );
    }

    #[test]
    fn nonce_fix_manual_fill_range_missing_end() {
        let e = get_nonce_fix_fill_range(NonceFixFillRangeInput::Manual {
            start: Some(1),
            end: None,
        })
        .unwrap_err();
        assert!(e.to_string().contains("manual mode requires --end"), "{e}");
    }

    #[test]
    fn nonce_fix_invalid_fill_range() {
        let e = get_nonce_fix_fill_range(NonceFixFillRangeInput::Manual {
            start: Some(5),
            end: Some(5),
        })
        .unwrap_err();
        assert!(
            e.to_string().contains("--end must be greater than --start"),
            "{e}"
        );

        let e = get_nonce_fix_fill_range(NonceFixFillRangeInput::Manual {
            start: Some(5),
            end: Some(3),
        })
        .unwrap_err();
        assert!(
            e.to_string().contains("--end must be greater than --start"),
            "{e}"
        );
    }

    #[test]
    fn nonce_fix_manual_fill_range() {
        let r = get_nonce_fix_fill_range(NonceFixFillRangeInput::Manual {
            start: Some(2),
            end: Some(5),
        })
        .unwrap();
        assert_eq!(r, Some(2..5));
    }

    #[test]
    fn nonce_fix_default_fee_cap() {
        let parent = TokenAmount::from_atto(100u64);
        let cap = get_nonce_fix_gas_fee_cap(None, parent.clone()).unwrap();
        assert_eq!(cap, parent * 2u64);
    }

    #[test]
    fn nonce_fix_explicit_fee_cap() {
        let parent = TokenAmount::from_atto(999u64);
        let cap = get_nonce_fix_gas_fee_cap(Some("42"), parent).unwrap();
        assert_eq!(cap, TokenAmount::from_atto(42u64));
    }

    #[test]
    fn nonce_fix_invalid_fee_cap() {
        let parent = TokenAmount::from_atto(1u64);
        let e = get_nonce_fix_gas_fee_cap(Some("not-a-number"), parent).unwrap_err();
        assert!(e.to_string().contains("invalid --gas-fee-cap value"), "{e}");
    }

    #[test]
    fn compute_statistics() {
        use crate::shim::message::Message;
        use fvm_ipld_encoding::RawBytes;
        use std::str::FromStr;

        let addr0 = Address::from_str("t3urxivigpzih5f6ih3oq3lr2jlunw3m5oehbe5efts4ub5wy2oi4fbo5cw7333a4rrffo5535tjdq24wkc2aa").unwrap();
        let addr1 = Address::from_str("t410fot3vkzzorqg4alowvghvxx4mhofhtazixbm6z2i").unwrap();
        let messages = [
            Message {
                version: 0,
                from: addr0,
                to: Address::default(),
                sequence: 1210,
                value: TokenAmount::default(),
                method_num: 5,
                params: RawBytes::new(vec![]),
                gas_limit: 25201703,
                gas_fee_cap: TokenAmount::from_atto(101774),
                gas_premium: TokenAmount::from_atto(100720),
            },
            Message {
                version: 0,
                from: addr1,
                to: Address::default(),
                sequence: 190,
                value: TokenAmount::default(),
                method_num: 5,
                params: RawBytes::new(vec![]),
                gas_limit: 21148671,
                gas_fee_cap: TokenAmount::from_atto(101774),
                gas_premium: TokenAmount::from_atto(100720),
            },
            Message {
                version: 0,
                from: addr1,
                to: Address::default(),
                sequence: 191,
                value: TokenAmount::default(),
                method_num: 5,
                params: RawBytes::new(vec![]),
                gas_limit: 112795625,
                gas_fee_cap: TokenAmount::from_atto(101774),
                gas_premium: TokenAmount::from_atto(100720),
            },
        ];
        let actor_sequences = HashMap::from_iter([(addr0, 1210), (addr1, 195)]);
        let curr_base_fee = TokenAmount::from_atto(100);
        let min_base_fee = TokenAmount::from_atto(100);

        let stats = compute_stats(&messages, actor_sequences, curr_base_fee, min_base_fee);

        let expected = vec![
            MpStat {
                address: addr0.to_string(),
                past: 0,
                current: 1,
                future: 0,
                below_current: 0,
                below_past: 0,
                gas_limit: 25201703.into(),
            },
            MpStat {
                address: addr1.to_string(),
                past: 2,
                current: 0,
                future: 0,
                below_current: 0,
                below_past: 0,
                gas_limit: 133944296.into(),
            },
        ];

        assert_eq!(stats, expected);
    }
}
