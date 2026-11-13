// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Early termination of a pledged CC sector through Forest's RPC and VM.
//!
//! lotus-miner locates the sector via Forest, then sends `TerminateSectors`. A
//! wrong-location `StateCall` mutation runs before the real terminate.
//!
//! Mirrors Lotus
//! [`TestTerminate`](https://github.com/filecoin-project/lotus/blob/v1.39.1/itests/sector_terminate_test.go#L35).
//! Terminates a pledged sector on the genesis miner (not a pre-seal), so block
//! production stays intact.

use super::api::TestMiner;
use super::devnet::{At, Devnet};
use crate::dev::subcommands::tests_cmd::helpers::{
    block_on, poll, poll_until_message_executed, trial,
};
use crate::rpc::prelude::*;
use crate::shim::address::Address;
use crate::shim::fvm_shared_latest::error::ExitCode;
use crate::shim::message::Message;
use crate::shim::policy::policy_constants::WPOST_PERIOD_DEADLINES;
use crate::shim::sector::SectorSize;
use anyhow::{Context as _, ensure};
use fil_actor_miner_state::v19::{Method, TerminateSectorsParams, TerminationDeclaration};
use fil_actors_shared::fvm_ipld_bitfield::BitField;
use fvm_ipld_encoding::RawBytes;
use libtest_mimic::Trial;
use num::BigInt;

pub(super) fn tests() -> Vec<Trial> {
    vec![trial("terminate::pledged_sector", || {
        block_on(pledged_sector())
    })]
}

async fn pledged_sector() -> anyhow::Result<()> {
    let net = Devnet::connect()?;
    let miner = TestMiner::genesis()?;

    let at = net.synced().await?;
    let before = at
        .same(|tsk| StateMinerPower::request((miner.actor, tsk)))
        .await?;
    let info = at
        .same(|tsk| StateMinerInfo::request((miner.actor, tsk)))
        .await?;
    ensure!(
        info.sector_size == SectorSize::_2KiB,
        "expected a 2 KiB miner, got {:?}",
        info.sector_size
    );
    let sector_power = BigInt::from(SectorSize::_2KiB as u64);

    let sector = miner.api.pledge_sector().await?;
    miner.api.wait_until_proving(sector).await?;

    let location = net
        .synced()
        .await?
        .same(|tsk| StateSectorPartition::request((miner.actor, sector, tsk)))
        .await?;
    let at = wait_for_sector_power(
        &net,
        miner.actor,
        sector,
        &before.miner_power.raw_byte_power,
    )
    .await?;
    let powered = at
        .same(|tsk| StateMinerPower::request((miner.actor, tsk)))
        .await?;
    let expected_powered = &before.miner_power.raw_byte_power + &sector_power;
    ensure!(
        powered.miner_power.raw_byte_power == expected_powered,
        "after proving sector {sector}, raw power is {}, expected {expected_powered}",
        powered.miner_power.raw_byte_power
    );

    // Same tipset for deadline/partition lookups lotus-miner will use.
    at.same(|tsk| StateMinerProvingDeadline::request((miner.actor, tsk)))
        .await?;
    let partitions = at
        .same(|tsk| StateMinerPartitions::request((miner.actor, location.deadline, tsk)))
        .await?;
    let partition = partitions
        .get(usize::try_from(location.partition)?)
        .with_context(|| {
            format!(
                "StateSectorPartition says partition {}, but deadline {} has {} partitions",
                location.partition,
                location.deadline,
                partitions.len()
            )
        })?;
    ensure!(
        partition.live_sectors.get(sector),
        "sector {sector} is not live in deadline {} partition {}",
        location.deadline,
        location.partition
    );

    // Wrong in-range location must fail in the VM, not via an OOB partition index.
    let (wrong_deadline, wrong_partition) = if partitions.len() > 1 {
        let idx = usize::try_from(location.partition)?;
        (
            location.deadline,
            u64::try_from((idx + 1) % partitions.len())?,
        )
    } else {
        let mut other = None;
        for deadline in 0..WPOST_PERIOD_DEADLINES {
            if deadline == location.deadline {
                continue;
            }
            let parts = at
                .same(|tsk| StateMinerPartitions::request((miner.actor, deadline, tsk)))
                .await?;
            if !parts.is_empty() {
                other = Some((deadline, 0));
                break;
            }
        }
        other.context(
            "deadline has one partition and no other non-empty deadline for the mutation check",
        )?
    };
    let mut wrong_sectors = BitField::new();
    wrong_sectors.set(sector);
    let wrong = Message::builder()
        .from(info.worker)
        .to(miner.actor)
        .method_num(Method::TerminateSectors as u64)
        .params(RawBytes::serialize(TerminateSectorsParams {
            terminations: vec![TerminationDeclaration {
                deadline: wrong_deadline,
                partition: wrong_partition,
                sectors: wrong_sectors,
            }],
        })?)
        .build();
    let invoc = at
        .same(|tsk| StateCall::request((wrong.clone(), tsk)))
        .await?;
    let receipt = invoc
        .msg_rct
        .context("no receipt for the wrong-location terminate StateCall")?;
    let rejected = invoc
        .error
        .contains("not all sectors are assigned to the partition")
        || invoc.error.contains("can only terminate live sectors")
        || invoc.error.contains("failed to find partition");
    ensure!(
        receipt.exit_code() == ExitCode::USR_ILLEGAL_ARGUMENT && rejected,
        "TerminateSectors at deadline {wrong_deadline} partition {wrong_partition} exits {} \
         with `{}`, expected {} with a sector/partition rejection; sector {sector} is in \
         deadline {} partition {}",
        receipt.exit_code(),
        invoc.error,
        ExitCode::USR_ILLEGAL_ARGUMENT,
        location.deadline,
        location.partition
    );

    miner.api.terminate_sector(sector).await?;
    let terminate_cid = miner.api.wait_until_terminated(sector).await?;
    let lookup = poll_until_message_executed(&net.forest, terminate_cid).await?;
    ensure!(
        lookup.receipt.exit_code().is_success(),
        "TerminateSectors {terminate_cid} exited {}",
        lookup.receipt.exit_code()
    );
    let at = net.synced().await?;
    let after = at
        .same(|tsk| StateMinerPower::request((miner.actor, tsk)))
        .await?;
    ensure!(
        after.miner_power.raw_byte_power == before.miner_power.raw_byte_power,
        "after terminating sector {sector}, raw power is {}, expected the pre-pledge {}",
        after.miner_power.raw_byte_power,
        before.miner_power.raw_byte_power
    );

    let partitions = at
        .same(|tsk| StateMinerPartitions::request((miner.actor, location.deadline, tsk)))
        .await?;
    let partition = partitions
        .get(usize::try_from(location.partition)?)
        .with_context(|| {
            format!(
                "deadline {} lost partition {} after terminate",
                location.deadline, location.partition
            )
        })?;
    ensure!(
        partition.all_sectors.get(sector),
        "sector {sector} missing from AllSectors after terminate"
    );
    ensure!(
        !partition.live_sectors.get(sector),
        "sector {sector} still in LiveSectors after terminate"
    );
    ensure!(
        !partition.active_sectors.get(sector),
        "sector {sector} still in ActiveSectors after terminate"
    );
    Ok(())
}

/// Waits until `miner`'s raw power is `baseline` plus one 2 KiB sector, waiting through the
/// sector's next deadline close when prove-commit has not added power yet.
async fn wait_for_sector_power<'a>(
    net: &'a Devnet,
    miner: Address,
    sector: u64,
    baseline: &BigInt,
) -> anyhow::Result<At<'a>> {
    let target = baseline + BigInt::from(SectorSize::_2KiB as u64);
    let at = net.synced().await?;
    let power = at
        .same(|tsk| StateMinerPower::request((miner, tsk)))
        .await?;
    if power.miner_power.raw_byte_power >= target {
        return Ok(at);
    }

    let location = at
        .same(|tsk| StateSectorPartition::request((miner, sector, tsk)))
        .await?;
    let current = at
        .same(|tsk| StateMinerProvingDeadline::request((miner, tsk)))
        .await?
        .0;
    let deadline = current.for_index(location.deadline).next_not_elapsed();
    eprintln!(
        "raw power still {}; waiting for deadline {} to close at epoch {} so WindowPoSt can claim sector {sector}",
        power.miner_power.raw_byte_power, location.deadline, deadline.close
    );
    net.wait_for_epoch(deadline.close).await?;
    poll(
        &format!(
            "raw power {target} after deadline {} for sector {sector}",
            location.deadline
        ),
        || async {
            let at = net.synced().await?;
            let power = at
                .same(|tsk| StateMinerPower::request((miner, tsk)))
                .await?;
            Ok((power.miner_power.raw_byte_power >= target).then_some(at))
        },
    )
    .await
}
