// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! A missing sector loses power, then `proving recover-faults` restores it.

use super::api::TestMiner;
use super::call;
use super::devnet::{At, Devnet};
use crate::dev::subcommands::tests_cmd::helpers::{block_on, docker, harness_env, poll, trial};
use crate::rpc::LOOKBACK_NO_LIMIT;
use crate::rpc::prelude::*;
use crate::rpc::types::{ApiTipsetKey, MessageFilter};
use crate::shim::actors::miner::{DeadlineInfo, MinerPower};
use crate::shim::address::{Address, Payload};
use crate::shim::clock::ChainEpoch;
use crate::shim::message::Message;
use anyhow::{Context as _, bail, ensure};
use fil_actor_miner_state::v19::{DeclareFaultsRecoveredParams, Method};
use libtest_mimic::Trial;

/// Hide this many epochs before the challenge. One `docker exec` can outrun a single epoch.
const CHALLENGE_MARGIN: ChainEpoch = 5;

pub(super) fn tests() -> Vec<Trial> {
    vec![trial("faults::skip_and_recover", || {
        block_on(skip_and_recover())
    })]
}

async fn skip_and_recover() -> anyhow::Result<()> {
    let net = Devnet::connect()?;
    let test_miner = TestMiner::genesis()?;
    let miner = test_miner.actor;
    let miner_id = miner_id(miner)?;

    let sector = test_miner.api.pledge_sector().await?;
    test_miner.api.wait_until_proving(sector).await?;

    let at = net.synced().await?;
    let index = at
        .same(|tsk| StateSectorPartition::request((miner, sector, tsk)))
        .await?
        .deadline;
    let power = at
        .same(|tsk| StateMinerPower::request((miner, tsk)))
        .await?;
    ensure!(
        power.has_min_power,
        "miner {miner} is below minimum power before the fault"
    );
    let faults = at
        .same(|tsk| StateMinerFaults::request((miner, tsk)))
        .await?;
    ensure!(
        !faults.get(sector),
        "sector {sector} is already in StateMinerFaults"
    );

    let deadline = fault_sector(&net, miner, miner_id, sector, index).await?;
    let at = wait_with_power(&net, miner, deadline.close).await?;
    assert_faulted(&at, miner, sector, &power).await?;

    let at = declare_recovery(&net, miner, miner_id, sector, index).await?;
    let recovered = deadline_ahead(&at, miner, index).await?;
    let at = wait_with_power(&net, miner, recovered.close).await?;
    assert_posts(&at, miner, &recovered).await?;
    assert_restored(&at, miner, &power).await?;
    Ok(())
}

async fn fault_sector(
    net: &Devnet,
    miner: Address,
    miner_id: u64,
    sector: u64,
    index: u64,
) -> anyhow::Result<DeadlineInfo> {
    let at = net.synced().await?;
    let deadline = deadline_ahead(&at, miner, index).await?;
    // The store index, not the files. Resolving it here keeps the margin for the rename.
    let paths = storage_find(miner_id, sector)?;
    let hide_at = deadline
        .challenge
        .checked_sub(CHALLENGE_MARGIN)
        .context("challenge epoch has no room for the hide margin")?;
    wait_forest_epoch(net, miner, hide_at).await?;
    move_sector_files(&paths, false).with_context(|| format!("hiding sector {sector}"))?;
    let head = call(&net.forest, "forest", ChainHead::request(())?)
        .await?
        .epoch();
    ensure!(
        head < deadline.challenge,
        "head {head} passed deadline {index} challenge {}; sector {sector} was already read",
        deadline.challenge
    );
    Ok(deadline)
}

/// `proving recover-faults` before Lotus's scheduler declares this recovery itself.
async fn declare_recovery<'a>(
    net: &'a Devnet,
    miner: Address,
    miner_id: u64,
    sector: u64,
    index: u64,
) -> anyhow::Result<At<'a>> {
    let at = net.synced().await?;
    let since = at.head.epoch();
    let paths = storage_find(miner_id, sector)?;
    move_sector_files(&paths, true).with_context(|| format!("restoring sector {sector}"))?;
    let at = net.synced().await?;
    ensure_before_auto_recover(&at, miner, index).await?;
    miner_exec(&[
        "lotus-miner".to_string(),
        "proving".to_string(),
        "recover-faults".to_string(),
        sector.to_string(),
    ])
    .context("lotus-miner proving recover-faults")?;
    let at = net.synced().await?;
    assert_recovery(&at, miner, sector, since).await?;
    Ok(at)
}

async fn deadline_ahead(at: &At<'_>, miner: Address, index: u64) -> anyhow::Result<DeadlineInfo> {
    let current = at
        .same(|tsk| StateMinerProvingDeadline::request((miner, tsk)))
        .await?
        .0;
    Ok(with_challenge_margin(current.for_index(index)))
}

fn with_challenge_margin(mut info: DeadlineInfo) -> DeadlineInfo {
    info = info.next_not_challenged();
    if info.challenge.saturating_sub(info.current_epoch) >= CHALLENGE_MARGIN {
        return info;
    }
    info.current_epoch = info.challenge;
    info.next_not_challenged()
}

async fn ensure_before_auto_recover(at: &At<'_>, miner: Address, index: u64) -> anyhow::Result<()> {
    let count = at
        .same(|tsk| StateMinerDeadlines::request((miner, tsk)))
        .await?
        .len();
    let count = u64::try_from(count).context("deadline count does not fit in u64")?;
    ensure!(count > 2, "miner {miner} has {count} deadlines");
    // While proving deadline `i`, Lotus declares recoveries for deadline `i + 2`.
    let auto = (index + count - 2) % count;
    let current = at
        .same(|tsk| StateMinerProvingDeadline::request((miner, tsk)))
        .await?
        .0;
    let auto_deadline = current.for_index(auto).next_not_challenged();
    let head = at.head.epoch();
    ensure!(
        head < auto_deadline.challenge,
        "head {head} has reached deadline {auto}'s challenge at {}, when Lotus declares recoveries for deadline {index} itself",
        auto_deadline.challenge
    );
    Ok(())
}

/// Waits until Forest's head reaches `epoch`. Does not wait for Lotus, so a caller can use the
/// epochs before the next `synced`.
async fn wait_forest_epoch(net: &Devnet, miner: Address, epoch: ChainEpoch) -> anyhow::Result<()> {
    let mut head = call(&net.forest, "forest", ChainHead::request(())?)
        .await?
        .epoch();
    while head < epoch {
        head = poll(
            &format!("forest to advance past epoch {head} towards {epoch}"),
            || async {
                let now = call(&net.forest, "forest", ChainHead::request(())?)
                    .await?
                    .epoch();
                let power = call(
                    &net.forest,
                    "forest",
                    StateMinerPower::request((miner, ApiTipsetKey(None)))?,
                )
                .await?;
                ensure!(
                    power.has_min_power,
                    "miner {miner} lost minimum power at epoch {now}; block production would stop"
                );
                Ok((now > head).then_some(now))
            },
        )
        .await?;
    }
    Ok(())
}

async fn wait_with_power<'a>(
    net: &'a Devnet,
    miner: Address,
    epoch: ChainEpoch,
) -> anyhow::Result<At<'a>> {
    wait_forest_epoch(net, miner, epoch).await?;
    let at = net.synced().await?;
    let power = at
        .same(|tsk| StateMinerPower::request((miner, tsk)))
        .await?;
    ensure!(
        power.has_min_power,
        "miner {miner} lost minimum power at epoch {}; block production would stop",
        at.head.epoch()
    );
    Ok(at)
}

async fn assert_faulted(
    at: &At<'_>,
    miner: Address,
    sector: u64,
    before: &MinerPower,
) -> anyhow::Result<()> {
    let faults = at
        .same(|tsk| StateMinerFaults::request((miner, tsk)))
        .await?;
    ensure!(
        faults.get(sector),
        "sector {sector} is not in StateMinerFaults"
    );
    let after = at
        .same(|tsk| StateMinerPower::request((miner, tsk)))
        .await?;
    ensure!(
        after.miner_power.raw_byte_power < before.miner_power.raw_byte_power
            && after.miner_power.quality_adj_power < before.miner_power.quality_adj_power,
        "power did not drop after sector {sector} faulted: {before:?} before, {after:?} after"
    );
    Ok(())
}

async fn assert_restored(at: &At<'_>, miner: Address, before: &MinerPower) -> anyhow::Result<()> {
    let faults = at
        .same(|tsk| StateMinerFaults::request((miner, tsk)))
        .await?;
    ensure!(
        faults.is_empty(),
        "sectors still faulted after recovery: {faults:?}"
    );
    let after = at
        .same(|tsk| StateMinerPower::request((miner, tsk)))
        .await?;
    ensure!(
        after.miner_power.raw_byte_power == before.miner_power.raw_byte_power
            && after.miner_power.quality_adj_power == before.miner_power.quality_adj_power
            && after.has_min_power,
        "power was not restored: {before:?} before, {after:?} after"
    );
    Ok(())
}

async fn assert_posts(at: &At<'_>, miner: Address, deadline: &DeadlineInfo) -> anyhow::Result<()> {
    let posts = super::submit_windowed_posts(at, miner, deadline).await?;
    ensure!(
        !posts.is_empty(),
        "no SubmitWindowedPoSt for deadline {} since epoch {}",
        deadline.index,
        deadline.open
    );
    for (cid, _) in posts {
        let exit = at
            .same(|tsk| StateSearchMsg::request((tsk, cid, LOOKBACK_NO_LIMIT, true)))
            .await?
            .with_context(|| format!("no node finds the PoSt {cid}"))?
            .receipt
            .exit_code();
        ensure!(exit.is_success(), "SubmitWindowedPoSt {cid} exited {exit}");
    }
    Ok(())
}

async fn assert_recovery(
    at: &At<'_>,
    miner: Address,
    sector: u64,
    since: ChainEpoch,
) -> anyhow::Result<()> {
    let to_miner = MessageFilter {
        from: None,
        to: Some(miner),
    };
    let mut found = false;
    let mut included = false;
    for cid in at
        .same(|tsk| StateListMessages::request((to_miner.clone(), tsk, since)))
        .await?
    {
        let message: Message = at.same(|_| ChainGetMessage::request((cid,))).await?;
        if message.method_num() != Method::DeclareFaultsRecovered as u64 {
            continue;
        }
        let params: DeclareFaultsRecoveredParams = message
            .params()
            .deserialize()
            .with_context(|| format!("decoding DeclareFaultsRecovered {cid}"))?;
        let exit = at
            .same(|tsk| StateSearchMsg::request((tsk, cid, LOOKBACK_NO_LIMIT, true)))
            .await?
            .with_context(|| format!("no node finds DeclareFaultsRecovered {cid}"))?
            .receipt
            .exit_code();
        ensure!(
            exit.is_success(),
            "DeclareFaultsRecovered {cid} exited {exit}"
        );
        found = true;
        included |= params
            .recoveries
            .iter()
            .any(|recovery| recovery.sectors.get(sector));
    }
    ensure!(
        found,
        "no DeclareFaultsRecovered to {miner} since epoch {since}"
    );
    ensure!(
        included,
        "no DeclareFaultsRecovered since epoch {since} includes sector {sector}"
    );
    Ok(())
}

fn miner_id(miner: Address) -> anyhow::Result<u64> {
    match miner.payload() {
        Payload::ID(id) => Ok(*id),
        _ => bail!("miner {miner} is not an id address"),
    }
}

fn storage_find(miner_id: u64, sector: u64) -> anyhow::Result<Vec<String>> {
    let output = miner_exec(&[
        "lotus-miner".to_string(),
        "storage".to_string(),
        "find".to_string(),
        sector.to_string(),
    ])
    .context("lotus-miner storage find")?;
    local_sector_paths(&output, miner_id, sector)
}

/// Sealed and cache paths from `storage find`. A `Remote` copy would still prove, so it is an error.
fn local_sector_paths(
    find_output: &str,
    miner_id: u64,
    sector: u64,
) -> anyhow::Result<Vec<String>> {
    let file = format!("s-t0{miner_id}-{sector}");
    let mut paths = Vec::new();
    for line in find_output.lines() {
        let line = line.trim();
        if line == "Remote" {
            bail!("storage find reports a remote copy of sector {sector}");
        }
        let Some(store) = line
            .strip_prefix("Local (")
            .and_then(|rest| rest.strip_suffix(')'))
        else {
            continue;
        };
        ensure!(
            !store.is_empty(),
            "storage find printed an empty local path for sector {sector}"
        );
        paths.push(format!("{store}/sealed/{file}"));
        paths.push(format!("{store}/cache/{file}"));
    }
    ensure!(
        !paths.is_empty(),
        "storage find printed no local path for sector {sector}"
    );
    Ok(paths)
}

fn move_sector_files(paths: &[String], restore: bool) -> anyhow::Result<()> {
    ensure!(!paths.is_empty(), "no sector files to move");
    let script = if restore {
        "set -eu; for path in \"$@\"; do mv -- \"$path.aside\" \"$path\"; done"
    } else {
        "set -eu; for path in \"$@\"; do mv -- \"$path\" \"$path.aside\"; done"
    };
    let mut args = vec![
        "sh".to_string(),
        "-c".to_string(),
        script.to_string(),
        "mv-sector".to_string(),
    ];
    args.extend(paths.iter().cloned());
    miner_exec(&args)?;
    Ok(())
}

/// `docker exec` does not inherit the miner's `FULLNODE_API_INFO`.
fn miner_exec(args: &[String]) -> anyhow::Result<String> {
    let port = harness_env("FOREST_RPC_PORT")?;
    let data_dir = harness_env("FOREST_DATA_DIR")?;
    let script = format!(
        "export FULLNODE_API_INFO=\"$(cat '{data_dir}/token.jwt'):/dns/forest/tcp/{port}/http\" && exec \"$@\""
    );
    let mut cmd = Vec::with_capacity(6 + args.len());
    cmd.push("exec".to_string());
    cmd.push("lotus-miner".to_string());
    cmd.push("sh".to_string());
    cmd.push("-c".to_string());
    cmd.push(script);
    cmd.push("miner-exec".to_string());
    cmd.extend_from_slice(args);
    let refs: Vec<&str> = cmd.iter().map(String::as_str).collect();
    docker(&refs)
}

#[cfg(test)]
mod tests {
    use super::local_sector_paths;

    #[test]
    fn local_sector_paths_join_sealed_and_cache() {
        let output = "\
In store-1 (Sealed, Cache)
\tSealing: true; Storage: true
\tLocal (/var/lib/lotus/storage)
\tURL: http://127.0.0.1/remote
";
        let paths = local_sector_paths(output, 1000, 7).unwrap();
        assert_eq!(
            paths,
            vec![
                "/var/lib/lotus/storage/sealed/s-t01000-7".to_string(),
                "/var/lib/lotus/storage/cache/s-t01000-7".to_string(),
            ]
        );
        let remote = "In store-1 (Sealed, Cache)\n\tRemote\n";
        let err = local_sector_paths(remote, 1000, 7).unwrap_err();
        assert!(
            err.to_string().contains("remote"),
            "unexpected error: {err}"
        );
    }
}
