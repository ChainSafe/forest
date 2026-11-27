// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! The lotus-miner operator commands through Forest's RPC. Each trial runs one command twice on
//! the same miner daemon, with Lotus and then with Forest as the command's full node, and compares
//! the two runs: the output follows the head, so the Lotus run is the expected one. Lotus's own
//! `TestMinerAllInfo` only checks that `info all` returns no error
//! (<https://github.com/filecoin-project/lotus/blob/v1.39.1/cli/miner/allinfo_test.go#L16-L45>).
//!
//! The trials follow the `onboarding` ones in their group, so a run of the group compares the
//! commands on a miner with a pledged sector.

use super::api::{Node, lotus_miner_exec};
use super::devnet::Devnet;
use crate::dev::subcommands::tests_cmd::helpers::{block_on, trial};
use crate::tool::subcommands::api_cmd::generate_diff;
use anyhow::bail;
use itertools::Itertools as _;
use libtest_mimic::Trial;
use serde_json::json;

/// Pairs of runs a comparison makes at most. A command reads its node's head, so the two runs of
/// a pair differ when the head moves between them; only such a pair is run again.
const ATTEMPTS: usize = 10;

pub(super) fn tests() -> Vec<Trial> {
    vec![
        trial("operator::proving_info", || {
            block_on(same_output(&["proving", "info"], str::to_owned))
        }),
        trial("operator::proving_deadlines", || {
            block_on(same_output(&["proving", "deadlines"], str::to_owned))
        }),
        trial("operator::proving_faults", || {
            block_on(same_output(&["proving", "faults"], str::to_owned))
        }),
        trial("operator::sectors_list", || {
            block_on(same_output(&["sectors", "list"], str::to_owned))
        }),
        trial("operator::actor_control_list", || {
            block_on(same_output(&["actor", "control", "list"], str::to_owned))
        }),
        trial("operator::info", || {
            block_on(same_output(&["info"], without_wall_clock))
        }),
        trial("operator::info_all", || {
            block_on(same_output(&["info", "all"], section_errors))
        }),
    ]
}

/// Fails unless `part` of what `lotus-miner <args>` prints is the same with Lotus and with Forest
/// as its full node: at once when a run exits non-zero or when the two runs differ and the head
/// epoch did not move during them, and after [`ATTEMPTS`] pairs of runs when it moved during each.
/// What a command prints may change with the head but not while the head stands still, so a
/// change of the miner daemon's own state during a pair fails the trial.
async fn same_output(args: &[&str], part: fn(&str) -> String) -> anyhow::Result<()> {
    let net = Devnet::connect()?;
    let command = args.join(" ");
    let mut diff = String::new();
    for _ in 0..ATTEMPTS {
        let epoch = net.synced().await?.epoch();
        let lotus = part(&lotus_miner_exec(Node::Lotus, args)?);
        let forest = part(&lotus_miner_exec(Node::Forest, args)?);
        if forest == lotus {
            return Ok(());
        }
        diff = generate_diff(
            &json!(forest.lines().collect_vec()),
            &json!(lotus.lines().collect_vec()),
        );
        if net.synced().await?.epoch() == epoch {
            bail!(
                "`lotus-miner {command}` differs between the nodes at epoch {epoch} (-forest +lotus):\n{diff}"
            );
        }
    }
    bail!(
        "`lotus-miner {command}`: the head moved during each of {ATTEMPTS} pairs of runs, the last one (-forest +lotus):\n{diff}"
    )
}

/// Returns what `info` prints without its wall-clock parts: the `StartTime: ` line, which counts
/// the seconds the daemon has run
/// (<https://github.com/filecoin-project/lotus/blob/v1.39.1/cli/miner/info.go#L84-L88>), and the
/// sync label that opens the `Chain: ` line, which follows the age of the head.
fn without_wall_clock(output: &str) -> String {
    output
        .lines()
        .filter(|line| !line.starts_with("StartTime: "))
        .map(|line| {
            match line
                .strip_prefix("Chain: [sync ")
                .and_then(|rest| rest.split_once("] "))
            {
                Some((_, base_fee)) => format!("Chain: {base_fee}"),
                None => line.to_owned(),
            }
        })
        .join("\n")
}

/// Returns the `ERROR: ` lines of `info all`, each after the header of its section, one per line
/// (<https://github.com/filecoin-project/lotus/blob/v1.39.1/cli/miner/info_all.go#L37-L200>).
/// Each node words its own errors (Forest's print as `RPC error (-32603): <text>`, Lotus's bare),
/// so a section in which both nodes reject a call differs.
fn section_errors(output: &str) -> String {
    let mut section = "";
    let mut errors = Vec::new();
    for line in output.lines() {
        if line.starts_with("#: ") || line.starts_with("##: ") {
            section = line;
            continue;
        }
        // Not `starts_with`: `info` prints `Chain: ` without a newline before its first call to
        // the full node, so that call's error follows on the same line
        // (<https://github.com/filecoin-project/lotus/blob/v1.39.1/cli/miner/info.go#L91-L96>).
        if line.contains("ERROR: ") {
            errors.push(format!("{section} {line}"));
        }
    }
    errors.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_errors_lists_each_error_line_after_its_section_header() {
        let output = [
            "#: Miner Info",
            "Enabled subsystems: [Mining Sealing SectorStorage]",
            "Chain: ERROR:  RPC error (-32601): Method not found",
            "",
            "#: Reachability",
            "AutoNAT status:  Unknown",
            "",
            "#: Expired Sectors",
            "ERROR:  too early to terminate sectors",
            "",
            "#: Per Sector Info",
            "",
            "##: Sector 0 Status",
            "ERROR:  RPC error (-32601): Method not found",
            "ERROR:  RPC error (-32601): Method not found",
            "",
            "##: Sector 0 Storage Location",
            "\tSealing: false; Storage: false",
        ]
        .join("\n");
        assert_eq!(
            section_errors(&output),
            [
                "#: Miner Info Chain: ERROR:  RPC error (-32601): Method not found",
                "#: Expired Sectors ERROR:  too early to terminate sectors",
                "##: Sector 0 Status ERROR:  RPC error (-32601): Method not found",
                "##: Sector 0 Status ERROR:  RPC error (-32601): Method not found",
            ]
            .join("\n")
        );
    }

    #[test]
    fn without_wall_clock_drops_the_start_time_line_and_the_sync_label() {
        let output = [
            "Enabled subsystems: [Mining Sealing SectorStorage]",
            "StartTime: 9m40s (started at 2026-11-18 11:55:40 +0000 UTC)",
            "Chain: [sync ok] [basefee 100 aFIL]",
            "⚠ 1 Active alerts (check lotus-miner log alerts)",
            "Miner: t01000 (2 KiB sectors)",
            "Power: 40 Ki / 40 Ki (100.0000%)",
        ]
        .join("\n");
        let behind = output
            .replace("9m40s", "9m41s")
            .replace("[sync ok]", "[sync behind! (1m55s behind)]");
        let expected = [
            "Enabled subsystems: [Mining Sealing SectorStorage]",
            "Chain: [basefee 100 aFIL]",
            "⚠ 1 Active alerts (check lotus-miner log alerts)",
            "Miner: t01000 (2 KiB sectors)",
            "Power: 40 Ki / 40 Ki (100.0000%)",
        ]
        .join("\n");
        assert_eq!(without_wall_clock(&output), expected);
        assert_eq!(without_wall_clock(&behind), expected);
    }
}
