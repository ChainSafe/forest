// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Curio seal/status/create-miner helpers for the Forest-produced Curio stack.

use crate::dev::subcommands::tests_cmd::helpers::{docker, harness_env, poll};
use crate::shim::address::Address;
use anyhow::{Context as _, bail};
use serde::Deserialize;
use serde_json::json;

/// Starts one CC sector via `curio seal start` and returns its number.
pub(super) async fn pledge_sector(actor: Address) -> anyhow::Result<u64> {
    let out = docker(&[
        "exec",
        "curio",
        "curio",
        "seal",
        "start",
        "--now",
        "--cc",
        "--count",
        "1",
        "--actor",
        &actor.to_string(),
    ])
    .context("`curio seal start` failed")?;
    let number = out
        .lines()
        .rev()
        .find_map(|l| l.trim().parse::<u64>().ok())
        .with_context(|| format!("no sector number in `curio seal start` output:\n{out}"))?;
    Ok(number)
}

/// Polls Harmony DB until the sector is committed.
pub(super) async fn wait_until_proving(actor: Address, sector: u64) -> anyhow::Result<()> {
    let sp_id = actor.id().context("miner actor is not an ID address")?;
    let waited = poll(&format!("Curio sector {sector} to finish sealing"), || async {
        let status = sector_status(sp_id, sector)?;
        match status {
            SectorStatus::Done => Ok(Some(())),
            SectorStatus::Failed {
                reason,
                reason_msg,
            } => {
                let events = seal_events(actor, sector).unwrap_or_default();
                bail!(
                    "Curio sector {sector} failed: {reason}: {reason_msg}\nseal events:\n{events}"
                );
            }
            SectorStatus::Unknown => bail!("sector {sector} unknown to Curio"),
            SectorStatus::InProgress => Ok(None),
        }
    })
    .await;
    if let Err(e) = waited {
        let events = seal_events(actor, sector).unwrap_or_default();
        return Err(e.context(format!("seal events:\n{events}")));
    }
    Ok(())
}

/// Creates a miner with `sptool` against Forest (empty peer id).
pub(super) fn create_miner(owner: Address, worker: Address) -> anyhow::Result<()> {
    let port = harness_env("FOREST_RPC_PORT")?;
    let data_dir = harness_env("FOREST_DATA_DIR")?;
    let script = format!(
        "export FULLNODE_API_INFO=\"$(cat '{data_dir}/token.jwt'):/dns/forest/tcp/{port}/http\" && \
         exec sptool --actor t01000 actor new-miner \
         --owner='{owner}' --worker='{worker}' --from='{owner}' \
         --sector-size 2KiB --confidence 1"
    );
    docker(&["exec", "curio", "sh", "-c", &script]).context("`sptool actor new-miner` failed")?;
    Ok(())
}

#[derive(Debug)]
enum SectorStatus {
    Done,
    Failed { reason: String, reason_msg: String },
    InProgress,
    Unknown,
}

#[derive(Debug, Deserialize)]
struct StatusRow {
    after_commit_msg_success: bool,
    failed: bool,
    failed_reason: String,
    failed_reason_msg: String,
    in_meta: bool,
    in_pipeline: bool,
}

fn sector_status(sp_id: u64, sector: u64) -> anyhow::Result<SectorStatus> {
    let sql = format!(
        "SELECT json_build_object(\
           'after_commit_msg_success', COALESCE(p.after_commit_msg_success IS NOT NULL, false),\
           'failed', COALESCE(p.failed, false),\
           'failed_reason', COALESCE(p.failed_reason, ''),\
           'failed_reason_msg', COALESCE(p.failed_reason_msg, ''),\
           'in_meta', EXISTS (SELECT 1 FROM curio.sectors_meta m \
               WHERE m.sp_id = {sp_id} AND m.sector_num = {sector}),\
           'in_pipeline', p.sp_id IS NOT NULL\
         ) FROM (SELECT {sp_id}::bigint AS sp_id, {sector}::bigint AS sector_number) s \
         LEFT JOIN curio.sectors_sdr_pipeline p \
           ON p.sp_id = s.sp_id AND p.sector_number = s.sector_number;"
    );
    let out = docker(&[
        "exec",
        "yugabyte",
        "bin/ysqlsh",
        "-h",
        "yugabyte",
        "-U",
        "yugabyte",
        "-At",
        "-c",
        &sql,
    ])
    .context("querying Curio sector status")?;
    let row: StatusRow = serde_json::from_str(out.trim())
        .with_context(|| format!("decoding Curio status JSON: {out}"))?;
    if row.failed {
        return Ok(SectorStatus::Failed {
            reason: row.failed_reason,
            reason_msg: row.failed_reason_msg,
        });
    }
    if row.after_commit_msg_success || (!row.in_pipeline && row.in_meta) {
        return Ok(SectorStatus::Done);
    }
    if !row.in_pipeline && !row.in_meta {
        return Ok(SectorStatus::Unknown);
    }
    Ok(SectorStatus::InProgress)
}

fn seal_events(actor: Address, sector: u64) -> anyhow::Result<String> {
    docker(&[
        "exec",
        "curio",
        "curio",
        "seal",
        "events",
        "--actor",
        &actor.to_string(),
        "--sector",
        &sector.to_string(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_row_decodes() {
        let row: StatusRow = serde_json::from_value(json!({
            "after_commit_msg_success": true,
            "failed": false,
            "failed_reason": "",
            "failed_reason_msg": "",
            "in_meta": true,
            "in_pipeline": true,
        }))
        .unwrap();
        assert!(row.after_commit_msg_success);
        assert!(row.in_meta);
        assert!(!row.failed);
    }
}
