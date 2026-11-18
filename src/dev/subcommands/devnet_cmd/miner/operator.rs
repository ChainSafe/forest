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

/// Pairs of runs a comparison tries before it fails. A command reads its node's head, so the two
/// runs of a pair differ when the head moves between them; a steady difference between the nodes
/// shows in every pair, one that comes and goes can pass.
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
        trial("operator::info", info),
        trial("operator::info_all", || {
            block_on(same_output(&["info", "all"], failed_sections))
        }),
    ]
}

/// Fails unless `part` of what `lotus-miner <args>` prints is the same with Lotus and with Forest
/// as its full node in one of [`ATTEMPTS`] pairs of runs, or at once when a run exits non-zero.
async fn same_output(args: &[&str], part: fn(&str) -> String) -> anyhow::Result<()> {
    let net = Devnet::connect()?;
    let mut diff = String::new();
    for _ in 0..ATTEMPTS {
        net.synced().await?;
        let lotus = part(&lotus_miner_exec(Node::Lotus, args)?);
        let forest = part(&lotus_miner_exec(Node::Forest, args)?);
        if forest == lotus {
            return Ok(());
        }
        diff = generate_diff(
            &json!(forest.lines().collect_vec()),
            &json!(lotus.lines().collect_vec()),
        );
    }
    bail!(
        "`lotus-miner {}` differs between the nodes in {ATTEMPTS} pairs of runs, the last one (-forest +lotus):\n{diff}",
        args.join(" ")
    )
}

/// Fails unless `lotus-miner info` exits 0 with each node. Its output says how long the daemon
/// has run, in seconds
/// (<https://github.com/filecoin-project/lotus/blob/v1.39.1/cli/miner/info.go#L84-L88>), so it is
/// not compared and a figure Forest answers differently passes.
fn info() -> anyhow::Result<()> {
    lotus_miner_exec(Node::Lotus, &["info"])?;
    lotus_miner_exec(Node::Forest, &["info"])?;
    Ok(())
}

/// Returns the headers of the `info all` sections that printed an error, one per line
/// (<https://github.com/filecoin-project/lotus/blob/v1.39.1/cli/miner/info_all.go#L37-L200>).
fn failed_sections(output: &str) -> String {
    let mut section = "";
    let mut failed = Vec::new();
    for line in output.lines() {
        if line.starts_with("#: ") || line.starts_with("##: ") {
            section = line;
            continue;
        }
        // Not `starts_with`: `info` prints `Chain: ` without a newline before its first call to
        // the full node, so that call's error follows on the same line
        // (<https://github.com/filecoin-project/lotus/blob/v1.39.1/cli/miner/info.go#L91-L96>).
        if line.contains("ERROR: ") && failed.last() != Some(&section) {
            failed.push(section);
        }
    }
    failed.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_sections_lists_each_section_with_an_error_once() {
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
            failed_sections(&output),
            "#: Miner Info\n#: Expired Sectors\n##: Sector 0 Status"
        );
    }
}
