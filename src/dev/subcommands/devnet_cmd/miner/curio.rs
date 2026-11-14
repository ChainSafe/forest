// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::forest_fullnode_api_info_shell;
use crate::dev::subcommands::tests_cmd::helpers::{docker, forest_client, harness_env, poll};
use crate::rpc::prelude::*;
use crate::rpc::types::ApiTipsetKey;
use crate::shim::address::Address;
use anyhow::{Context as _, bail};
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};

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
    parse_seal_start_sector(&out)
}

/// Harmony-DB "Done" can lead the chain; still wait for Forest `StateSectorGetInfo`.
pub(super) async fn wait_until_proving(actor: Address, sector: u64) -> anyhow::Result<()> {
    let sp_id = actor.id().context("miner actor is not an ID address")?;
    let forest = forest_client()?;
    let harmony_done = AtomicBool::new(false);
    poll(
        &format!("Curio sector {sector} sealed and on Forest"),
        || async {
            let status = if harmony_done.load(Ordering::Relaxed) {
                SectorStatus::Done
            } else {
                let status = tokio::task::spawn_blocking(move || sector_status(sp_id, sector))
                    .await
                    .context("sector_status join")??;
                if matches!(status, SectorStatus::Done) {
                    harmony_done.store(true, Ordering::Relaxed);
                }
                status
            };
            match status {
                SectorStatus::Failed {
                    reason,
                    reason_msg,
                } => {
                    let events = seal_events(actor, sector).unwrap_or_default();
                    bail!(
                        "Curio sector {sector} failed: {reason}: {reason_msg}\nseal events:\n{events}"
                    );
                }
                SectorStatus::Unknown | SectorStatus::InProgress => Ok(None),
                SectorStatus::Done => {
                    let info = forest
                        .call(StateSectorGetInfo::request((
                            actor,
                            sector,
                            ApiTipsetKey(None),
                        ))?)
                        .await
                        .with_context(|| format!("StateSectorGetInfo for sector {sector}"))?;
                    Ok(info.map(|_| ()))
                }
            }
        },
    )
    .await
    .map_err(|e| {
        let events = seal_events(actor, sector).unwrap_or_default();
        e.context(format!("seal events:\n{events}"))
    })?;
    Ok(())
}

pub(super) fn create_miner(owner: Address, worker: Address) -> anyhow::Result<()> {
    let api = forest_fullnode_api_info_shell()?;
    // `sptool` needs a global `--actor` even for `new-miner` (genesis SP = CLI context).
    let actor = harness_env("MINER_ACTOR_ADDRESS")?;
    let script = format!(
        "export FULLNODE_API_INFO=\"{api}\" && \
         exec sptool --actor {actor} actor new-miner \
         --owner='{owner}' --worker='{worker}' --from='{owner}' \
         --sector-size 2KiB --confidence 1"
    );
    docker(&["exec", "curio", "sh", "-c", &script]).context("`sptool actor new-miner` failed")?;
    Ok(())
}

/// Prefer a pure-numeric line from `curio seal start`, else the last integer token.
fn parse_seal_start_sector(out: &str) -> anyhow::Result<u64> {
    if let Some(n) = out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .rev()
        .find_map(|l| l.parse().ok())
    {
        return Ok(n);
    }
    out.split_whitespace()
        .rev()
        .find_map(|tok| tok.trim_matches(|c: char| !c.is_ascii_digit()).parse().ok())
        .with_context(|| format!("no sector number in `curio seal start` output:\n{out}"))
}

#[derive(Debug, PartialEq, Eq)]
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
    // after_commit_msg_success / failed are NOT NULL booleans on sectors_sdr_pipeline.
    let sql = format!(
        "SELECT json_build_object(\
           'after_commit_msg_success', COALESCE(p.after_commit_msg_success, false),\
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
    Ok(status_from_row(row))
}

fn status_from_row(row: StatusRow) -> SectorStatus {
    if row.failed {
        return SectorStatus::Failed {
            reason: row.failed_reason,
            reason_msg: row.failed_reason_msg,
        };
    }
    if row.after_commit_msg_success || (!row.in_pipeline && row.in_meta) {
        return SectorStatus::Done;
    }
    if !row.in_pipeline && !row.in_meta {
        return SectorStatus::Unknown;
    }
    SectorStatus::InProgress
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

    fn row(
        after_commit_msg_success: bool,
        failed: bool,
        in_meta: bool,
        in_pipeline: bool,
    ) -> StatusRow {
        StatusRow {
            after_commit_msg_success,
            failed,
            failed_reason: if failed { "boom".into() } else { String::new() },
            failed_reason_msg: if failed {
                "details".into()
            } else {
                String::new()
            },
            in_meta,
            in_pipeline,
        }
    }

    #[test]
    fn status_from_row_branches() {
        assert_eq!(
            status_from_row(row(false, true, false, true)),
            SectorStatus::Failed {
                reason: "boom".into(),
                reason_msg: "details".into(),
            }
        );
        assert_eq!(
            status_from_row(row(true, false, true, true)),
            SectorStatus::Done
        );
        assert_eq!(
            status_from_row(row(false, false, true, false)),
            SectorStatus::Done
        );
        assert_eq!(
            status_from_row(row(false, false, false, false)),
            SectorStatus::Unknown
        );
        assert_eq!(
            status_from_row(row(false, false, false, true)),
            SectorStatus::InProgress
        );
        assert_eq!(
            status_from_row(row(false, false, true, true)),
            SectorStatus::InProgress
        );
    }

    #[test]
    fn parse_seal_start_uses_last_numeric_line() {
        let out = "starting seal\nnoise 99\n\n42\ndone\n";
        assert_eq!(parse_seal_start_sector(out).unwrap(), 42);
    }

    #[test]
    fn parse_seal_start_accepts_sector_in_prose() {
        assert_eq!(
            parse_seal_start_sector("Started sealing sector 42\n").unwrap(),
            42
        );
    }

    #[test]
    fn parse_seal_start_rejects_when_no_numeric_line() {
        let err = parse_seal_start_sector("ok\ndone").unwrap_err().to_string();
        assert!(err.contains("no sector number"), "unexpected error: {err}");
    }
}
