// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
pub mod store;
mod weight;
use crate::blocks::{Tipset, TipsetKey};
use crate::cid_collections::CidHashSet;
use crate::db::car::forest::{self, ForestCarFrame, finalize_frame};
use crate::db::{SettingsStore, SettingsStoreExt};
use crate::ipld::stream_chain;
use crate::lotus_json::lotus_json_with_self;
use crate::utils::db::car_stream::{CarBlock, CarBlockWrite};
use crate::utils::io::{AsyncWriterWithChecksum, Checksum};
use crate::utils::multihash::MultihashCode;
use crate::utils::stream::par_buffer;
use anyhow::Context as _;
use cid::Cid;
use digest::Digest;
use futures::StreamExt as _;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::DAG_CBOR;
use itertools::Itertools as _;
use multihash_derive::MultihashDigest as _;
use num::FromPrimitive as _;
use num_derive::FromPrimitive;
use nunny::Vec as NonEmpty;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::sync::Arc;
use tokio::io::{AsyncWrite, AsyncWriteExt, BufWriter};

pub use self::{store::*, weight::*};

pub async fn export_from_head<D: Digest>(
    db: &Arc<impl Blockstore + SettingsStore + Send + Sync + 'static>,
    lookup_depth: ChainEpochDelta,
    writer: impl AsyncWrite + Unpin,
    seen: CidHashSet,
    skip_checksum: bool,
) -> anyhow::Result<(Tipset, Option<digest::Output<D>>), Error> {
    let head_key = SettingsStoreExt::read_obj::<TipsetKey>(db, crate::db::setting_keys::HEAD_KEY)?
        .context("chain head key not found")?;
    let head_ts = Tipset::load_required(&db, &head_key)?;
    let digest = export::<D>(db, &head_ts, lookup_depth, writer, seen, skip_checksum).await?;
    Ok((head_ts, digest))
}

pub async fn export<D: Digest>(
    db: &Arc<impl Blockstore + Send + Sync + 'static>,
    tipset: &Tipset,
    lookup_depth: ChainEpochDelta,
    writer: impl AsyncWrite + Unpin,
    seen: CidHashSet,
    skip_checksum: bool,
) -> anyhow::Result<Option<digest::Output<D>>, Error> {
    let roots = tipset.key().to_cids();
    export_to_forest_car::<D>(
        roots,
        None,
        db,
        tipset,
        lookup_depth,
        writer,
        seen,
        skip_checksum,
    )
    .await
}

pub async fn export_v2<D: Digest>(
    db: &Arc<impl Blockstore + Send + Sync + 'static>,
    f3: Option<(Cid, File)>,
    tipset: &Tipset,
    lookup_depth: ChainEpochDelta,
    writer: impl AsyncWrite + Unpin,
    seen: CidHashSet,
    skip_checksum: bool,
) -> anyhow::Result<Option<digest::Output<D>>, Error> {
    let head = tipset.key().to_cids();
    let f3_cid = f3.as_ref().map(|(cid, _)| *cid);
    let snap_meta = FilecoinSnapshotMetadata::new_v2(head, f3_cid);
    let snap_meta_cbor_encoded = fvm_ipld_encoding::to_vec(&snap_meta)?;
    let snap_meta_block = CarBlock {
        cid: Cid::new_v1(
            DAG_CBOR,
            MultihashCode::Blake2b256.digest(&snap_meta_cbor_encoded),
        ),
        data: snap_meta_cbor_encoded,
    };
    let roots = nunny::vec![snap_meta_block.cid];
    let mut prefix_data_frames = vec![{
        let mut encoder = forest::new_encoder(forest::DEFAULT_FOREST_CAR_COMPRESSION_LEVEL)?;
        snap_meta_block.write(&mut encoder)?;
        anyhow::Ok((
            vec![snap_meta_block.cid],
            finalize_frame(forest::DEFAULT_FOREST_CAR_COMPRESSION_LEVEL, &mut encoder)?,
        ))
    }];

    if let Some((f3_cid, mut f3_data)) = f3 {
        prefix_data_frames.push({
            let mut encoder = forest::new_encoder(forest::DEFAULT_FOREST_CAR_COMPRESSION_LEVEL)?;
            encoder.write_car_block(f3_cid, f3_data.metadata()?.len() as _, &mut f3_data)?;
            anyhow::Ok((
                vec![f3_cid],
                finalize_frame(forest::DEFAULT_FOREST_CAR_COMPRESSION_LEVEL, &mut encoder)?,
            ))
        });
    }

    export_to_forest_car::<D>(
        roots,
        Some(prefix_data_frames),
        db,
        tipset,
        lookup_depth,
        writer,
        seen,
        skip_checksum,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn export_to_forest_car<D: Digest>(
    roots: NonEmpty<Cid>,
    prefix_data_frames: Option<Vec<anyhow::Result<ForestCarFrame>>>,
    db: &Arc<impl Blockstore + Send + Sync + 'static>,
    tipset: &Tipset,
    lookup_depth: ChainEpochDelta,
    writer: impl AsyncWrite + Unpin,
    seen: CidHashSet,
    skip_checksum: bool,
) -> anyhow::Result<Option<digest::Output<D>>, Error> {
    let stateroot_lookup_limit = tipset.epoch() - lookup_depth;

    // Wrap writer in optional checksum calculator
    let mut writer = AsyncWriterWithChecksum::<D, _>::new(BufWriter::new(writer), !skip_checksum);

    // Stream stateroots in range (stateroot_lookup_limit+1)..=tipset.epoch(). Also
    // stream all block headers until genesis.
    let blocks = par_buffer(
        // Queue 1k blocks. This is enough to saturate the compressor and blocks
        // are small enough that keeping 1k in memory isn't a problem. Average
        // block size is between 1kb and 2kb.
        1024,
        stream_chain(
            Arc::clone(db),
            tipset.clone().chain_owned(Arc::clone(db)),
            stateroot_lookup_limit,
        )
        .with_seen(seen),
    );

    // Encode Ipld key-value pairs in zstd frames
    let block_frames = forest::Encoder::compress_stream_default(blocks);
    let frames = futures::stream::iter(prefix_data_frames.unwrap_or_default()).chain(block_frames);

    // Write zstd frames and include a skippable index
    forest::Encoder::write(&mut writer, roots, frames).await?;

    // Flush to ensure everything has been successfully written
    writer.flush().await.context("failed to flush")?;

    let digest = writer.finalize().map_err(|e| Error::Other(e.to_string()))?;

    Ok(digest)
}

#[derive(Debug, Copy, clap::ValueEnum, FromPrimitive, Clone, PartialEq, Eq, JsonSchema)]
#[repr(u64)]
pub enum FilecoinSnapshotVersion {
    V1 = 1,
    V2 = 2,
}
lotus_json_with_self!(FilecoinSnapshotVersion);

impl Serialize for FilecoinSnapshotVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u64(*self as u64)
    }
}

impl<'de> Deserialize<'de> for FilecoinSnapshotVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let i = u64::deserialize(deserializer)?;
        match FilecoinSnapshotVersion::from_u64(i) {
            Some(v) => Ok(v),
            None => Err(serde::de::Error::custom(format!(
                "invalid snapshot version {i}"
            ))),
        }
    }
}

/// Defined in <https://github.com/filecoin-project/FIPs/blob/master/FRCs/frc-0108.md#snapshotmetadata>
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct FilecoinSnapshotMetadata {
    /// Snapshot version
    pub version: FilecoinSnapshotVersion,
    /// Chain head tipset key
    pub head_tipset_key: NonEmpty<Cid>,
    /// F3 snapshot `CID`
    pub f3_data: Option<Cid>,
}

impl FilecoinSnapshotMetadata {
    pub fn new(
        version: FilecoinSnapshotVersion,
        head_tipset_key: NonEmpty<Cid>,
        f3_data: Option<Cid>,
    ) -> Self {
        Self {
            version,
            head_tipset_key,
            f3_data,
        }
    }

    pub fn new_v2(head_tipset_key: NonEmpty<Cid>, f3_data: Option<Cid>) -> Self {
        Self::new(FilecoinSnapshotVersion::V2, head_tipset_key, f3_data)
    }
}

impl std::fmt::Display for FilecoinSnapshotMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f, "Snapshot version: {}", self.version as u64)?;
        let head_tipset_key_string = self
            .head_tipset_key
            .iter()
            .map(Cid::to_string)
            .join("\n                  ");
        writeln!(f, "Head Tipset:      {head_tipset_key_string}")?;
        write!(
            f,
            "F3 data:          {}",
            self.f3_data
                .map(|c| c.to_string())
                .unwrap_or_else(|| "not found".into())
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use sha2::{Digest as _, Sha256};

    use super::*;
    use crate::{
        blocks::{CachingBlockHeader, Chain4U, Tipset, TipsetKey, chain4u},
        db::{MemoryDB, car::ForestCar},
        utils::db::CborStoreExt,
    };

    #[test]
    fn test_snapshot_version_cbor_serde() {
        assert_eq!(
            fvm_ipld_encoding::to_vec(&FilecoinSnapshotVersion::V2),
            fvm_ipld_encoding::to_vec(&2_u64)
        );
        assert_eq!(
            fvm_ipld_encoding::from_slice::<FilecoinSnapshotVersion>(
                &fvm_ipld_encoding::to_vec(&2_u64).unwrap()
            )
            .unwrap(),
            FilecoinSnapshotVersion::V2
        );
    }

    #[tokio::test]
    async fn test_export_v1() {
        test_export_inner(FilecoinSnapshotVersion::V1)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn test_export_v2() {
        test_export_inner(FilecoinSnapshotVersion::V2)
            .await
            .unwrap()
    }

    async fn test_export_inner(version: FilecoinSnapshotVersion) -> anyhow::Result<()> {
        let db = Arc::new(MemoryDB::default());
        let c4u = Chain4U::with_blockstore(db.clone());
        chain4u! {
            in c4u; // select the context
            [genesis]
            -> [b_1]
            -> [b_2_0, b_2_1]
            -> [b_3]
            -> [b_4]
            -> [b_5_0, b_5_1]
        };

        let head_key_cids = nunny::vec![b_5_0.cid(), b_5_1.cid()];
        let head_key = TipsetKey::from(head_key_cids.clone());
        let head = Tipset::load_required(&db, &head_key)?;

        let mut car_bytes = vec![];

        let checksum = match version {
            FilecoinSnapshotVersion::V1 => {
                export::<Sha256>(&db, &head, 0, &mut car_bytes, Default::default(), false).await?
            }
            FilecoinSnapshotVersion::V2 => {
                export_v2::<Sha256>(
                    &db,
                    None,
                    &head,
                    0,
                    &mut car_bytes,
                    Default::default(),
                    false,
                )
                .await?
            }
        };

        assert_eq!(Sha256::digest(&car_bytes), checksum.unwrap());

        let car = ForestCar::new(car_bytes)?;

        assert_eq!(car.heaviest_tipset()?, head);

        match version {
            FilecoinSnapshotVersion::V1 => {
                assert_eq!(car.metadata(), &None);
            }
            FilecoinSnapshotVersion::V2 => {
                assert_eq!(
                    car.metadata(),
                    &Some(FilecoinSnapshotMetadata {
                        version,
                        head_tipset_key: head_key_cids,
                        f3_data: None,
                    })
                );
            }
        }

        for b in [&genesis, &b_1, &b_2_0, &b_2_1, &b_3, &b_4, &b_5_0, &b_5_1] {
            let b_from_car: CachingBlockHeader = car.get_cbor_required(&b.cid())?;
            let b_from_db: CachingBlockHeader = db.get_cbor_required(&b.cid())?;
            assert_eq!(b_from_car, b_from_db);
        }

        Ok(())
    }
}
