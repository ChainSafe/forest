// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(test)]
mod tests;

use crate::{rpc::f3::`F3`PowerEntry, utils::multihash::MultihashCode};
use cid::Cid;
use fvm_ipld_encoding::{IPLD_RAW, tuple::*};
use integer_encoding::VarIntReader as _;
use std::io::Read;

pub fn get_f3_snapshot_cid(f3_data: &mut impl Read) -> anyhow::Result<Cid> {
    Ok(Cid::new_v1(
        IPLD_RAW,
        MultihashCode::Blake2b256.digest_byte_stream(f3_data)?,
    ))
}

/// Defined in <https://github.com/filecoin-project/FIPs/blob/98e33b9fa306959aa0131519eb4cc155522b2081/FRCs/frc-0108.md#f3snapshotheader>
#[derive(Debug, Clone, Eq, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct `F3`SnapshotHeader {
    pub version: u64,
    pub first_instance: u64,
    pub latest_instance: u64,
    pub initial_power_table: Vec<`F3`PowerEntry>,
}

impl `F3`SnapshotHeader {
    pub fn decode_from_snapshot(f3_snapshot: &mut impl Read) -> anyhow::Result<Self> {
        // Reasonable upper bound for snapshot header size (100MiB)
        const MAX_HEADER_SIZE: usize = 100 * 1024 * 1024;

        let data_len = f3_snapshot.read_varint::<usize>()?;
        anyhow::ensure!(
            data_len <= MAX_HEADER_SIZE,
            "`F3` snapshot header size {data_len} exceeds maximum allowed size {MAX_HEADER_SIZE}"
        );
        let mut data_bytes = vec![0; data_len];
        f3_snapshot.read_exact(&mut data_bytes)?;
        Ok(fvm_ipld_encoding::from_slice(&data_bytes)?)
    }
}

impl std::fmt::Display for `F3`SnapshotHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f, "`F3` snapshot version:        {}", self.version)?;
        writeln!(f, "`F3` snapshot first instance: {}", self.first_instance)?;
        write!(f, "`F3` snapshot last instance:  {}", self.latest_instance)
    }
}
