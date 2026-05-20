// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::Tipset;
use crate::cli::humantoken;
use crate::cli_shared::cli::FeeConfig;
use crate::lotus_json::{HasLotusJson as _, NotNullVec};
use crate::message::{MessageRead as _, SignedMessage};
use crate::message_pool::compute_rbf_min_premium;
use crate::rpc::gas::cap_gas_fee;
use crate::rpc::{self, prelude::*, types::ApiTipsetKey, types::MessageSendSpec};
use crate::shim::address::StrictAddress;
use crate::shim::message::{METHOD_SEND, Message};
use crate::shim::{address::Address, econ::TokenAmount};

use ahash::{HashMap, HashSet};
use anyhow::Context as _;
use cid::Cid;
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
        /// Address to fill nonce's for (must be signable by the node's wallet).
        #[arg(long)]
        addr: StrictAddress,
        /// Derive the fill range from chain state and the mempool (ignores `--start` / `--end`).
        #[arg(long, conflicts_with_all = ["start", "end"])]
        auto: bool,
        /// First sequence to fill (inclusive); required unless `--auto`.
        #[arg(long, required_unless_present = "auto")]
        start: Option<u64>,
        /// End of range (exclusive); required unless `--auto`.
        #[arg(long, required_unless_present = "auto")]
        end: Option<u64>,
        /// Gas fee cap for filler messages. Default: twice the parent base fee from chain head.
        #[arg(long, value_parser = humantoken::parse)]
        gas_fee_cap: Option<TokenAmount>,
    },
    /// Replace a pending message in the mempool with updated gas parameters (replace-by-fee).
    Replace {
        /// Address that sent the message (required unless `--cid` is used).
        #[arg(long, required_unless_present = "cid")]
        from: Option<StrictAddress>,
        /// Nonce of the message to replace (required unless `--cid` is used).
        #[arg(long, required_unless_present = "cid")]
        nonce: Option<u64>,
        /// CID of the message to replace (alternative to `--from`/`--nonce`).
        #[arg(long, conflicts_with_all = ["from", "nonce"])]
        cid: Option<Cid>,
        /// Automatically re-estimate gas, ensuring the RBF minimum premium is met.
        #[arg(long)]
        auto: bool,
        /// Maximum total fee; only used with `--auto`.
        #[arg(long, value_parser = humantoken::parse, alias = "fee-limit")]
        max_fee: Option<TokenAmount>,
        /// Gas premium (manual mode).
        #[arg(long, value_parser = humantoken::parse)]
        gas_premium: Option<TokenAmount>,
        /// Gas fee cap (manual mode).
        #[arg(long, value_parser = humantoken::parse)]
        gas_feecap: Option<TokenAmount>,
        /// Gas limit (manual mode; keeps original value if unset).
        #[arg(long)]
        gas_limit: Option<u64>,
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

fn get_gas_fee_cap(gas_fee_cap: Option<TokenAmount>, parent_base_fee: TokenAmount) -> TokenAmount {
    gas_fee_cap.unwrap_or_else(|| parent_base_fee * 2u64)
}

fn find_pending_message(
    from: Address,
    nonce: u64,
    pending: &[SignedMessage],
) -> anyhow::Result<SignedMessage> {
    pending
        .iter()
        .find(|m| m.from() == from && m.sequence() == nonce)
        .cloned()
        .with_context(|| format!("no pending message found from {from} with nonce {nonce}"))
}

enum ReplaceGasInput {
    Auto {
        estimated_msg: Message,
        original_premium: TokenAmount,
    },
    Manual {
        gas_premium: TokenAmount,
        gas_feecap: TokenAmount,
        gas_limit: Option<u64>,
        original_msg: Message,
    },
}

fn compute_replacement_gas(input: ReplaceGasInput) -> anyhow::Result<Message> {
    match input {
        ReplaceGasInput::Auto {
            mut estimated_msg,
            original_premium,
        } => {
            let min_premium = compute_rbf_min_premium(&original_premium);
            if estimated_msg.gas_premium < min_premium {
                estimated_msg.gas_premium = min_premium;
            }
            if estimated_msg.gas_fee_cap < estimated_msg.gas_premium {
                estimated_msg.gas_fee_cap = estimated_msg.gas_premium.clone();
            }
            Ok(estimated_msg)
        }
        ReplaceGasInput::Manual {
            gas_premium,
            gas_feecap,
            gas_limit,
            mut original_msg,
        } => {
            let min_premium = compute_rbf_min_premium(&original_msg.gas_premium);
            if gas_premium < min_premium {
                return Err(anyhow::anyhow!(
                    "gas premium is below the minimum required for RBF"
                ));
            }
            original_msg.gas_premium = gas_premium;
            original_msg.gas_fee_cap = gas_feecap;
            if let Some(limit) = gas_limit {
                original_msg.gas_limit = limit;
            }
            Ok(original_msg)
        }
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
                let fee_cap = get_gas_fee_cap(gas_fee_cap, parent_base_fee);
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
            Self::Replace {
                from,
                nonce,
                cid,
                auto,
                max_fee,
                gas_premium,
                gas_feecap,
                gas_limit,
            } => {
                let (sender, sequence) = if let Some(msg_cid) = cid {
                    let api_msg = ChainGetMessage::call(&client, (msg_cid,)).await?;
                    (api_msg.from, api_msg.sequence)
                } else {
                    let sender: Address = from
                        .context("--from is required when --cid is not provided")?
                        .into();
                    let seq = nonce.context("--nonce is required when --cid is not provided")?;
                    (sender, seq)
                };

                let tipset = ChainHead::call(&client, ()).await?;
                let tsk = ApiTipsetKey(Some(tipset.key().clone()));

                let NotNullVec(pending) = MpoolPending::call(&client, (tsk,)).await?;
                let found = find_pending_message(sender, sequence, &pending)?;
                let original_msg = found.into_message();

                let msg_send_spec = Some(MessageSendSpec {
                    max_fee: max_fee.unwrap_or_default(),
                    msg_uuid: uuid::Uuid::nil(),
                    maximize_fee_cap: false,
                });

                let replacement = if auto {
                    let mut msg_for_estimate = original_msg.clone();
                    msg_for_estimate.gas_limit = 0;
                    msg_for_estimate.gas_fee_cap = TokenAmount::default();
                    msg_for_estimate.gas_premium = TokenAmount::default();

                    let estimated_msg = GasEstimateMessageGas::call(
                        &client,
                        (msg_for_estimate, msg_send_spec.clone(), ApiTipsetKey(None)),
                    )
                    .await?;

                    let mut replacement = compute_replacement_gas(ReplaceGasInput::Auto {
                        estimated_msg,
                        original_premium: original_msg.gas_premium,
                    })?;
                    cap_gas_fee(
                        &FeeConfig::default().max_fee,
                        &mut replacement,
                        msg_send_spec,
                    )?;
                    replacement
                } else {
                    let gas_premium =
                        gas_premium.context("--gas-premium is required unless --auto is set")?;
                    let gas_feecap =
                        gas_feecap.context("--gas-feecap is required unless --auto is set")?;
                    compute_replacement_gas(ReplaceGasInput::Manual {
                        gas_premium,
                        gas_feecap,
                        gas_limit,
                        original_msg,
                    })?
                };

                let smsg = WalletSignMessage::call(&client, (sender, replacement)).await?;
                let new_cid = MpoolPush::call(&client, (smsg,)).await?;
                println!("new message cid: {new_cid}");

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

    struct TestAddrs {
        addr: Address,
        target: Address,
        other: Address,
    }

    fn test_wallet() -> (Wallet, TestAddrs) {
        let keystore = KeyStore::new(KeyStoreConfig::Memory).unwrap();
        let mut wallet = Wallet::new(keystore);
        let addr = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let target = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        let other = wallet.generate_addr(SignatureType::Secp256k1).unwrap();
        (
            wallet,
            TestAddrs {
                addr,
                target,
                other,
            },
        )
    }

    fn pending_from(
        wallet: &mut Wallet,
        target: &Address,
        from: &Address,
        nonces: &[u64],
    ) -> Vec<SignedMessage> {
        nonces
            .iter()
            .map(|&nonce| create_smsg(target, from, wallet.borrow_mut(), nonce, 1_000_000, 1))
            .collect()
    }

    fn make_test_message(
        from: Address,
        to: Address,
        nonce: u64,
        gas_limit: u64,
        gas_premium: u64,
        gas_fee_cap: u64,
    ) -> Message {
        Message {
            version: 0,
            from,
            to,
            sequence: nonce,
            value: TokenAmount::default(),
            method_num: METHOD_SEND,
            params: RawBytes::new(vec![]),
            gas_limit,
            gas_fee_cap: TokenAmount::from_atto(gas_fee_cap),
            gas_premium: TokenAmount::from_atto(gas_premium),
        }
    }

    #[test]
    fn nonce_fix_fill_range_auto() {
        struct Case {
            name: &'static str,
            next_on_chain: u64,
            addr_nonces: &'static [u64],
            other_sender_nonce: Option<u64>,
            expected: Option<Range<u64>>,
        }

        let cases = [
            Case {
                name: "empty_pool",
                next_on_chain: 0,
                addr_nonces: &[],
                other_sender_nonce: None,
                expected: None,
            },
            Case {
                name: "wrong_sender",
                next_on_chain: 5,
                addr_nonces: &[],
                other_sender_nonce: Some(10),
                expected: None,
            },
            Case {
                name: "gap",
                next_on_chain: 5,
                addr_nonces: &[7],
                other_sender_nonce: None,
                expected: Some(5..7),
            },
            Case {
                name: "min_pending_nonce",
                next_on_chain: 5,
                addr_nonces: &[10, 8],
                other_sender_nonce: None,
                expected: Some(5..8),
            },
            Case {
                name: "next_nonce_in_mpool",
                next_on_chain: 5,
                addr_nonces: &[5],
                other_sender_nonce: None,
                expected: None,
            },
            Case {
                name: "ignores_stale_pending",
                next_on_chain: 5,
                addr_nonces: &[3, 9],
                other_sender_nonce: None,
                expected: Some(5..9),
            },
            Case {
                name: "only_stale_pending",
                next_on_chain: 5,
                addr_nonces: &[3],
                other_sender_nonce: None,
                expected: None,
            },
        ];

        for case in cases {
            let (mut wallet, addrs) = test_wallet();
            let mut pending =
                pending_from(&mut wallet, &addrs.target, &addrs.addr, case.addr_nonces);
            if let Some(nonce) = case.other_sender_nonce {
                pending.push(create_smsg(
                    &addrs.target,
                    &addrs.other,
                    wallet.borrow_mut(),
                    nonce,
                    1_000_000,
                    1,
                ));
            }
            let got = get_nonce_fix_fill_range(NonceFixFillRangeInput::Auto {
                addr: addrs.addr,
                next_on_chain_nonce: case.next_on_chain,
                pending,
            })
            .unwrap();
            assert_eq!(got, case.expected, "case {}", case.name);
        }
    }

    #[test]
    fn nonce_fix_fill_range_manual() {
        for (start, end, err) in [
            (None, Some(10), Some("manual mode requires --start")),
            (Some(1), None, Some("manual mode requires --end")),
            (Some(5), Some(5), Some("--end must be greater than --start")),
            (Some(5), Some(3), Some("--end must be greater than --start")),
        ] {
            let e = get_nonce_fix_fill_range(NonceFixFillRangeInput::Manual { start, end })
                .unwrap_err();
            assert!(
                e.to_string().contains(err.unwrap()),
                "start={start:?} end={end:?}: {e}"
            );
        }

        let r = get_nonce_fix_fill_range(NonceFixFillRangeInput::Manual {
            start: Some(2),
            end: Some(5),
        })
        .unwrap();
        assert_eq!(r, Some(2..5));
    }

    #[test]
    fn nonce_fix_gas_fee_cap() {
        let parent = TokenAmount::from_atto(100u64);
        assert_eq!(get_gas_fee_cap(None, parent.clone()), parent.clone() * 2u64);
        assert_eq!(
            get_gas_fee_cap(Some(TokenAmount::from_atto(42u64)), parent),
            TokenAmount::from_atto(42u64)
        );
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

    #[test]
    fn find_pending_message_lookup() {
        let (mut wallet, addrs) = test_wallet();
        let pending = pending_from(&mut wallet, &addrs.target, &addrs.addr, &[5]);

        let found = find_pending_message(addrs.addr, 5, &pending).unwrap();
        assert_eq!(found.cid(), pending[0].cid());

        for (from, nonce) in [(addrs.addr, 99), (addrs.other, 5)] {
            let err = find_pending_message(from, nonce, &pending).unwrap_err();
            assert!(
                err.to_string().contains("no pending message found"),
                "{err}"
            );
        }

        let err = find_pending_message(addrs.addr, 5, &[]).unwrap_err();
        assert!(
            err.to_string().contains("no pending message found"),
            "{err}"
        );
    }

    #[test]
    fn compute_replacement_gas_auto() {
        let (_wallet, addrs) = test_wallet();
        let addr = addrs.addr;
        let target = addrs.target;

        // Above RBF floor: estimated premium kept.
        let original_premium = TokenAmount::from_atto(100u64);
        let floor = compute_rbf_min_premium(&original_premium);
        let estimated = make_test_message(addr, target, 5, 2_000_000, 200, 500);
        assert!(estimated.gas_premium > floor);
        let result = compute_replacement_gas(ReplaceGasInput::Auto {
            estimated_msg: estimated.clone(),
            original_premium: original_premium.clone(),
        })
        .unwrap();
        assert_eq!(result.gas_premium, estimated.gas_premium);

        // Below RBF floor: premium bumped, fee cap >= premium.
        let original_premium = TokenAmount::from_atto(1000u64);
        let floor = compute_rbf_min_premium(&original_premium);
        let estimated = make_test_message(addr, target, 5, 2_000_000, 50, 500);
        assert!(estimated.gas_premium < floor);
        let result = compute_replacement_gas(ReplaceGasInput::Auto {
            estimated_msg: estimated,
            original_premium: original_premium.clone(),
        })
        .unwrap();
        assert_eq!(result.gas_premium, floor);
        assert!(result.gas_fee_cap >= result.gas_premium);

        // Exactly at floor: unchanged.
        let original_premium = TokenAmount::from_atto(100u64);
        let floor = compute_rbf_min_premium(&original_premium);
        let mut estimated = make_test_message(addr, target, 5, 2_000_000, 0, 500);
        estimated.gas_premium = floor.clone();
        estimated.gas_fee_cap = floor.clone();
        let result = compute_replacement_gas(ReplaceGasInput::Auto {
            estimated_msg: estimated,
            original_premium: original_premium.clone(),
        })
        .unwrap();
        assert_eq!(result.gas_premium, floor);
        assert_eq!(result.gas_fee_cap, floor);

        // Fee cap raised when below bumped premium.
        let original_premium = TokenAmount::from_atto(1000u64);
        let floor = compute_rbf_min_premium(&original_premium);
        let mut estimated = make_test_message(addr, target, 5, 2_000_000, 50, 10);
        estimated.gas_premium = floor.clone();
        let result = compute_replacement_gas(ReplaceGasInput::Auto {
            estimated_msg: estimated,
            original_premium,
        })
        .unwrap();
        assert_eq!(result.gas_premium, floor);
        assert_eq!(result.gas_fee_cap, floor);

        // cap_gas_fee after RBF bump.
        let original_premium = TokenAmount::from_atto(1_000_000u64);
        let mut estimated = make_test_message(addr, target, 5, 2_000_000, 50, 10_000_000_000);
        estimated.gas_premium = TokenAmount::from_atto(50u64);
        let mut replacement = compute_replacement_gas(ReplaceGasInput::Auto {
            estimated_msg: estimated,
            original_premium,
        })
        .unwrap();
        let max_fee = TokenAmount::from_atto(1_000_000u64);
        cap_gas_fee(&max_fee, &mut replacement, None).unwrap();
        let total_fee = replacement.gas_fee_cap.clone() * replacement.gas_limit;
        assert!(total_fee <= max_fee);
        assert!(replacement.gas_premium <= replacement.gas_fee_cap);
    }

    #[test]
    fn compute_replacement_gas_manual() {
        let (_wallet, addrs) = test_wallet();
        let addr = addrs.addr;
        let target = addrs.target;

        let original = make_test_message(addr, target, 5, 1_000_000, 100, 300);
        let result = compute_replacement_gas(ReplaceGasInput::Manual {
            gas_premium: TokenAmount::from_atto(200u64),
            gas_feecap: TokenAmount::from_atto(600u64),
            gas_limit: None,
            original_msg: original.clone(),
        })
        .unwrap();
        assert_eq!(result.gas_premium, TokenAmount::from_atto(200u64));
        assert_eq!(result.gas_fee_cap, TokenAmount::from_atto(600u64));
        assert_eq!(result.gas_limit, original.gas_limit);

        let original = make_test_message(addr, target, 5, 1_000_000, 100, 300);
        let min_premium = compute_rbf_min_premium(&original.gas_premium);
        let result = compute_replacement_gas(ReplaceGasInput::Manual {
            gas_premium: min_premium,
            gas_feecap: TokenAmount::from_atto(300u64),
            gas_limit: Some(5_000_000),
            original_msg: original,
        })
        .unwrap();
        assert_eq!(result.gas_limit, 5_000_000);

        let original = make_test_message(addr, target, 5, 1_000_000, 1000, 3000);
        let min_premium = compute_rbf_min_premium(&original.gas_premium);
        let below = min_premium - TokenAmount::from_atto(1u64);
        let e = compute_replacement_gas(ReplaceGasInput::Manual {
            gas_premium: below,
            gas_feecap: TokenAmount::from_atto(5000u64),
            gas_limit: None,
            original_msg: original,
        })
        .unwrap_err();
        assert!(
            e.to_string()
                .contains("gas premium is below the minimum required for RBF"),
            "{e}"
        );
    }
}
