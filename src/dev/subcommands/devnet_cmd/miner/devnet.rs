// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! The two full nodes of the devnet and the pinned comparison the trials assert with: Forest
//! answers, and Lotus, which validates the chain Forest produces, must answer the same.

use crate::blocks::Tipset;
use crate::dev::subcommands::tests_cmd::helpers::{
    forest_client, lotus_client, poll, poll_until_message_executed, poll_with,
};
use crate::lotus_json::HasLotusJson;
use crate::rpc::prelude::*;
use crate::rpc::types::{ApiTipsetKey, MessageLookup};
use crate::rpc::{Client, LOOKBACK_NO_LIMIT, Request, humanize_rpc_error};
use crate::shim::clock::ChainEpoch;
use crate::tool::subcommands::api_cmd::generate_diff;
use anyhow::{Context as _, bail, ensure};
use cid::Cid;
use std::fmt::Debug;
use std::time::Duration;

/// Forest under test and the Lotus node that validates the chain Forest produces.
pub(super) struct Devnet {
    pub forest: Client,
    pub lotus: Client,
}

impl Devnet {
    pub fn connect() -> anyhow::Result<Self> {
        Ok(Self {
            forest: forest_client()?,
            lotus: lotus_client()?,
        })
    }

    /// Waits until Lotus has Forest's current head and returns that head as the tipset to compare at.
    ///
    /// # Errors
    /// Lotus holds a different tipset at Forest's head epoch, or has marked one of its blocks bad;
    /// both fail at once instead of retrying, since they mean Lotus rejected what Forest produced.
    /// Lotus not reaching that epoch within the poll budget also fails.
    pub async fn synced(&self) -> anyhow::Result<At<'_>> {
        let head = call(&self.forest, "forest", ChainHead::request(())?).await?;
        poll(
            &format!("lotus to reach epoch {}", head.epoch()),
            || async {
                let lotus_head = call(&self.lotus, "lotus", ChainHead::request(())?).await?;
                Ok((lotus_head.epoch() >= head.epoch()).then_some(()))
            },
        )
        .await?;
        let lotus_at_height = call(
            &self.lotus,
            "lotus",
            ChainGetTipSetByHeight::request((head.epoch(), ApiTipsetKey(None)))?,
        )
        .await?;
        ensure!(
            lotus_at_height.key() == head.key(),
            "Lotus tipset at Forest head epoch {} differs: forest {}, lotus {}",
            head.epoch(),
            head.key(),
            lotus_at_height.key()
        );
        for cid in head.cids() {
            let reason = call(&self.lotus, "lotus", SyncCheckBad::request((cid,))?).await?;
            ensure!(
                reason.is_empty(),
                "Lotus rejected Forest block {cid} at epoch {}: {reason}",
                head.epoch()
            );
        }
        Ok(At { devnet: self, head })
    }

    /// Waits until Forest's head reaches `epoch`, then returns [`Self::synced`].
    ///
    /// # Errors
    /// The head does not reach `epoch` within one and a half block delays per epoch to go plus
    /// five minutes (the devnet runs slower than its block delay: about 5.5 s per 4 s epoch on
    /// CI), or [`Self::synced`] fails.
    pub async fn wait_for_epoch(&self, epoch: ChainEpoch) -> anyhow::Result<At<'_>> {
        let version = call(&self.forest, "forest", Version::request(())?).await?;
        let block_delay = Duration::from_secs(version.block_delay.into());
        let head = call(&self.forest, "forest", ChainHead::request(())?).await?;
        let epochs = u32::try_from(epoch - head.epoch()).unwrap_or(0);
        let budget = block_delay * epochs * 3 / 2 + Duration::from_secs(5 * 60);
        poll_with(
            &format!("forest to reach epoch {epoch}"),
            budget,
            block_delay,
            || async {
                let head = call(&self.forest, "forest", ChainHead::request(())?).await?;
                Ok((head.epoch() >= epoch).then_some(()))
            },
        )
        .await?;
        self.synced().await
    }

    /// Waits until Forest has executed `cid` and returns its lookup, which both nodes must report
    /// alike at a tipset both have.
    ///
    /// # Errors
    /// Forest does not execute `cid` within the 10 min poll budget (its `StateWaitMsg` looks back
    /// 800 epochs), [`Self::synced`] fails, or the nodes report the message differently.
    pub async fn landed(&self, cid: Cid) -> anyhow::Result<(At<'_>, MessageLookup)> {
        poll_until_message_executed(&self.forest, cid).await?;
        let at = self.synced().await?;
        let lookup = at
            .same(|tsk| StateSearchMsg::request((tsk, cid, LOOKBACK_NO_LIMIT, true)))
            .await?
            .with_context(|| {
                format!(
                    "Forest executed {cid}, but no node finds it at epoch {}",
                    at.head.epoch()
                )
            })?;
        Ok((at, lookup))
    }
}

/// A tipset both nodes have; every comparison through it is pinned to that tipset.
pub(super) struct At<'a> {
    devnet: &'a Devnet,
    head: Tipset,
}

impl At<'_> {
    /// Calls the request, built with this tipset's key, on both nodes and returns the answer they agree on.
    ///
    /// # Errors
    /// Either node fails the call, or the answers differ; the message then carries a line diff of the two.
    pub async fn same<T>(
        &self,
        request: impl Fn(ApiTipsetKey) -> serde_json::Result<Request<T>>,
    ) -> anyhow::Result<T>
    where
        T: HasLotusJson + PartialEq + Debug,
    {
        let key = ApiTipsetKey(Some(self.head.key().clone()));
        let forest_request = request(key.clone())?;
        let lotus_request = request(key)?;
        let method = forest_request.method_name.clone();
        let (forest, lotus) = tokio::try_join!(
            call(&self.devnet.forest, "forest", forest_request),
            call(&self.devnet.lotus, "lotus", lotus_request),
        )?;
        if forest == lotus {
            return Ok(forest);
        }
        let diff = generate_diff(
            &forest.into_lotus_json_value()?,
            &lotus.into_lotus_json_value()?,
        );
        bail!(
            "{method} at epoch {} differs (-forest +lotus):\n{diff}",
            self.head.epoch()
        )
    }
}

/// Calls `request` on `node`, naming the node and the method in the error.
async fn call<T: HasLotusJson + Debug>(
    client: &Client,
    node: &str,
    request: Request<T>,
) -> anyhow::Result<T> {
    let method = request.method_name.clone();
    client
        .call(request)
        .await
        .with_context(|| format!("{node} {method}"))
        .map_err(humanize_rpc_error)
}
