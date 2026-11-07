// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::call;
use crate::blocks::Tipset;
use crate::dev::subcommands::tests_cmd::helpers::{
    forest_client, lotus_client, poll, poll_until_message_executed,
};
use crate::lotus_json::HasLotusJson;
use crate::rpc::prelude::*;
use crate::rpc::types::{ApiTipsetKey, MessageLookup};
use crate::rpc::{Client, LOOKBACK_NO_LIMIT, Request};
use crate::shim::clock::ChainEpoch;
use crate::tool::subcommands::api_cmd::generate_diff;
use anyhow::{Context as _, bail, ensure};
use cid::Cid;
use std::fmt::Debug;

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

    /// Waits until Lotus has Forest's current head, failing at once if Lotus rejected one of
    /// Forest's blocks, and returns that head as the tipset to compare at.
    pub async fn synced(&self) -> anyhow::Result<At<'_>> {
        let head = call(&self.forest, "forest", ChainHead::request(())?).await?;
        poll(
            &format!("lotus to reach epoch {}", head.epoch()),
            || async {
                for cid in head.cids() {
                    let reason = call(&self.lotus, "lotus", SyncCheckBad::request((cid,))?).await?;
                    ensure!(
                        reason.is_empty(),
                        "Lotus rejected Forest block {cid} at epoch {}: {reason}",
                        head.epoch()
                    );
                }
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
        Ok(At { devnet: self, head })
    }

    /// Waits until Forest's head reaches `epoch`, failing if it stops advancing, then returns
    /// [`Self::synced`].
    pub async fn wait_for_epoch(&self, epoch: ChainEpoch) -> anyhow::Result<At<'_>> {
        let mut head = call(&self.forest, "forest", ChainHead::request(())?)
            .await?
            .epoch();
        while head < epoch {
            head = poll(
                &format!("forest to advance past epoch {head} towards {epoch}"),
                || async {
                    let now = call(&self.forest, "forest", ChainHead::request(())?)
                        .await?
                        .epoch();
                    Ok((now > head).then_some(now))
                },
            )
            .await?;
        }
        self.synced().await
    }

    /// Waits until Forest has executed `cid`, then returns the lookup both nodes agree on at a
    /// tipset both have.
    pub async fn executed(&self, cid: Cid) -> anyhow::Result<MessageLookup> {
        poll_until_message_executed(&self.forest, cid).await?;
        self.synced()
            .await?
            .same(|tsk| StateSearchMsg::request((tsk, cid, LOOKBACK_NO_LIMIT, true)))
            .await?
            .with_context(|| format!("Forest executed {cid}, but no node finds it"))
    }
}

/// A tipset both nodes have; every comparison through it is pinned to that tipset.
pub(super) struct At<'a> {
    devnet: &'a Devnet,
    pub(super) head: Tipset,
}

impl At<'_> {
    /// Calls the request, built with this tipset's key, on both nodes and returns the answer they agree on.
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
