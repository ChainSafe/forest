// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Mpool CLI integration tests (shared preloaded address).
//!
//! Run the mpool suite before the wallet suite; see `mise test:wallet`.
//! Each test assumes the same environment as the wallet suite.

use std::str::FromStr as _;

use super::helpers::*;
use crate::rpc::types::MessageSendSpec;
use crate::rpc::{RpcMethodExt as _, prelude::MpoolPushMessage};
use crate::shim::address::Address;
use crate::shim::econ::TokenAmount;
use crate::shim::message::Message;
use libtest_mimic::{Arguments, Trial};

/// Mpool integration tests
#[derive(Debug, clap::Args)]
pub struct MpoolTestCommand {}

impl MpoolTestCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        let args = Arguments {
            test_threads: Some(1),
            ..Default::default()
        };
        libtest_mimic::run(&args, tests()).exit();
    }
}

fn tests() -> Vec<Trial> {
    vec![
        Trial::test("mpool_nonce_fix_auto_unblocks_pending", || {
            block_on(mpool_nonce_fix_auto_unblocks_pending());
            Ok(())
        }),
        Trial::test("mpool_replace_auto_unblocks_pending", || {
            block_on(mpool_replace_auto_unblocks_pending());
            Ok(())
        }),
        Trial::test("mpool_push_message_dedups_by_msg_uuid", || {
            block_on(mpool_push_message_dedups_by_msg_uuid());
            Ok(())
        }),
    ]
}

async fn mpool_nonce_fix_auto_unblocks_pending() {
    let addr = FOREST_TEST_PRELOADED_ADDRESS.as_str();
    let nonce = mpool_nonce(addr).unwrap();
    // Skip one nonce so `--auto` has a gap to fill.
    let next_nonce = nonce + 1;
    forest_cli(&[
        "mpool",
        "nonce-fix",
        "--addr",
        addr,
        "--start",
        &next_nonce.to_string(),
        "--end",
        &(next_nonce + 1).to_string(),
    ])
    .unwrap();
    poll_until_pending_nonce(addr, next_nonce).await.unwrap();

    forest_cli(&["mpool", "nonce-fix", "--addr", addr, "--auto"]).unwrap();

    assert!(
        poll_until_pending_nonce(addr, nonce).await.is_ok(),
        "nonce-fix --auto should fill nonce gap at {nonce} for {addr}."
    );
}

async fn mpool_replace_auto_unblocks_pending() {
    let addr = FOREST_TEST_PRELOADED_ADDRESS.as_str();
    let nonce = mpool_nonce(addr).unwrap();

    let cid = send_from_no_wait(addr, addr, FIL_AMT, Backend::Local).unwrap();
    poll_until_pending_nonce(addr, nonce).await.unwrap();

    forest_cli(&[
        "mpool",
        "replace",
        "--from",
        addr,
        "--nonce",
        &nonce.to_string(),
        "--auto",
    ])
    .unwrap();

    assert!(
        poll_until_state_search_msg(&cid).await.is_ok(),
        "mpool replace --auto should replace message {cid} from {addr} at nonce {nonce}."
    );
}

/// Same `MsgUuid` retries share one CID; two distinct UUIDs advance nonce by 2.
async fn mpool_push_message_dedups_by_msg_uuid() {
    let addr_str = FOREST_TEST_PRELOADED_ADDRESS.as_str();
    let addr = Address::from_str(addr_str).unwrap();
    let client = forest_client().unwrap();
    let nonce_before = mpool_nonce(addr_str).unwrap();

    let make_msg = || {
        Message::builder()
            .from(addr)
            .to(addr)
            .value(TokenAmount::from_atto(1))
            .build()
    };
    let push = |uuid| {
        let client = &client;
        let make_msg = &make_msg;
        async move {
            MpoolPushMessage::call(client, (make_msg(), Some(MessageSendSpec::with_uuid(uuid))))
                .await
        }
    };

    let uuid_seq = crate::utils::rand::new_uuid_v4();
    let first = push(uuid_seq).await.unwrap();
    let second = push(uuid_seq).await.unwrap();
    assert_eq!(
        first.cid(),
        second.cid(),
        "sequential retries with the same MsgUuid must return the same CID"
    );

    let uuid_conc = crate::utils::rand::new_uuid_v4();
    let (left, right) = tokio::join!(push(uuid_conc), push(uuid_conc));
    assert_eq!(
        left.unwrap().cid(),
        right.unwrap().cid(),
        "concurrent calls with the same MsgUuid must return the same CID"
    );

    let nonce_after = mpool_nonce(addr_str).unwrap();
    assert_eq!(
        nonce_after,
        nonce_before + 2,
        "exactly two unique MsgUuids must advance the nonce by 2"
    );
}
