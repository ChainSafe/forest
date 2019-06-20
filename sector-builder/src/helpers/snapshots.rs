use std::sync::Arc;

use byteorder::{LittleEndian, WriteBytesExt};
use filecoin_proofs::types::PaddedBytesAmount;

use crate::builder::WrappedKeyValueStore;
use crate::error::Result;
use crate::kv_store::KeyValueStore;
use crate::state::*;

pub struct SnapshotKey {
    prover_id: [u8; 31],
    sector_size: PaddedBytesAmount,
}

impl SnapshotKey {
    pub fn new(prover_id: [u8; 31], sector_size: PaddedBytesAmount) -> SnapshotKey {
        SnapshotKey {
            prover_id,
            sector_size,
        }
    }
}

pub fn load_snapshot<T: KeyValueStore>(
    kv_store: &Arc<WrappedKeyValueStore<T>>,
    key: &SnapshotKey,
) -> Result<Option<SectorBuilderState>> {
    let result: Option<Vec<u8>> = kv_store.inner().get(&Vec::from(key))?;

    if let Some(val) = result {
        return serde_cbor::from_slice(&val[..])
            .map_err(failure::Error::from)
            .map(Option::Some);
    }

    Ok(None)
}

impl From<&SnapshotKey> for Vec<u8> {
    fn from(n: &SnapshotKey) -> Self {
        // convert the sector size to a byte vector
        let mut snapshot_key = Vec::with_capacity(n.prover_id.len() + 8);
        snapshot_key
            .write_u64::<LittleEndian>(u64::from(n.sector_size))
            .unwrap();

        // concatenate the prover id bytes
        snapshot_key.extend_from_slice(&n.prover_id[..]);

        snapshot_key
    }
}

pub fn persist_snapshot<T: KeyValueStore>(
    kv_store: &Arc<WrappedKeyValueStore<T>>,
    key: &SnapshotKey,
    state: &SectorBuilderState,
) -> Result<()> {
    let serialized = serde_cbor::to_vec(state)?;
    kv_store.inner().put(&Vec::from(key), &serialized)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::builder::{SectorId, WrappedKeyValueStore};
    use crate::kv_store::SledKvs;
    use crate::metadata::StagedSectorMetadata;
    use crate::state::StagedState;

    use super::*;

    #[test]
    fn test_snapshotting() {
        let metadata_dir = tempfile::tempdir().unwrap();

        let kv_store = Arc::new(WrappedKeyValueStore::new(
            SledKvs::initialize(metadata_dir).unwrap(),
        ));

        // create a snapshot to persist and load
        let snapshot_a = {
            let mut m: HashMap<SectorId, StagedSectorMetadata> = HashMap::new();

            m.insert(123, Default::default());

            let staged_state = StagedState {
                sector_id_nonce: 100,
                sectors: m,
            };

            let sealed_state = Default::default();

            SectorBuilderState {
                staged: staged_state,
                sealed: sealed_state,
            }
        };

        // create a second (different) snapshot
        let snapshot_b = {
            let mut m: HashMap<SectorId, StagedSectorMetadata> = HashMap::new();

            m.insert(666, Default::default());

            let staged_state = StagedState {
                sector_id_nonce: 102,
                sectors: m,
            };

            let sealed_state = Default::default();

            SectorBuilderState {
                staged: staged_state,
                sealed: sealed_state,
            }
        };

        let key_a = SnapshotKey::new([0; 31], PaddedBytesAmount(1024));
        let key_b = SnapshotKey::new([0; 31], PaddedBytesAmount(1111));
        let key_c = SnapshotKey::new([1; 31], PaddedBytesAmount(1024));

        // persist both snapshots
        let _ = persist_snapshot(&kv_store, &key_a, &snapshot_a).unwrap();
        let _ = persist_snapshot(&kv_store, &key_b, &snapshot_b).unwrap();

        // load both snapshots
        let loaded_a = load_snapshot(&kv_store, &key_a).unwrap().unwrap();
        let loaded_b = load_snapshot(&kv_store, &key_b).unwrap().unwrap();

        // key corresponds to no snapshot
        let lookup_miss = load_snapshot(&kv_store, &key_c).unwrap();

        assert_eq!(snapshot_a, loaded_a);
        assert_eq!(snapshot_b, loaded_b);
        assert_eq!(true, lookup_miss.is_none());
    }
}
