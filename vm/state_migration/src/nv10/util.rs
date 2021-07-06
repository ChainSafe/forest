// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::MigrationError;
use crate::MigrationResult;
use actor::{ipld_amt::Amt, make_empty_map};
use actor_interface::ActorVersion;
use actor_interface::{Array, Map as Map2};
use cid::Cid;
use forest_hash_utils::BytesKey;
use ipld_blockstore::BlockStore;
use serde::{de::DeserializeOwned, Serialize};

// Migrates a HAMT from v2 to v3 without re-encoding keys or values.
pub(crate) fn migrate_hamt_raw<
    BS: BlockStore,
    V: Clone + Serialize + PartialEq + DeserializeOwned,
>(
    store: &BS,
    root: Cid,
    new_bitwidth: u32,
) -> MigrationResult<Cid> {
    let in_root_node = Map2::load(&root, store, ActorVersion::V2)
        .map_err(|e| MigrationError::HAMTLoad(e.to_string()))?;

    let mut out_root_node = make_empty_map(store, new_bitwidth);

    in_root_node
        .for_each(|k: &BytesKey, v: &V| {
            // TODO: see if a set_raw API can be implemented to put plain bytes without having to deserialize it.
            out_root_node.set(k.clone(), v.clone())?;
            Ok(())
        })
        .map_err(|e| MigrationError::MigrateHAMT(e.to_string()))?;

    let root_cid = out_root_node.flush().map_err(|_| {
        MigrationError::FlushFailed("nv10 migration: hamt flush failed".to_string()).into()
    });

    root_cid
}

// Migrates an AMT from v2 to v3 without re-encoding values.
pub(crate) fn migrate_amt_raw<
    BS: BlockStore,
    V: Clone + Serialize + PartialEq + DeserializeOwned,
>(
    store: &BS,
    root: Cid,
    new_bitwidth: i32,
) -> Result<Cid, Box<dyn std::error::Error>> {
    let in_root_node = Array::load(&root, store, ActorVersion::V2)?;

    let mut out_root_node = Amt::new_with_bit_width(store, new_bitwidth as usize);

    in_root_node.for_each(|k: u64, v: &V| {
        // TODO: see if a set_raw API can be implemented to put plain bytes without having to deserialize it.
        out_root_node.set(k as usize, v.clone())?;
        Ok(())
    })?;

    let root_cid = out_root_node.flush().map_err(|_| {
        MigrationError::FlushFailed("nv10 migration: amt flush failed".to_string()).into()
    });

    root_cid
}

pub(crate) fn migrate_hamt_hamt_raw<
    BS: BlockStore,
    V: Clone + Serialize + PartialEq + DeserializeOwned,
>(
    store: &BS,
    root: Cid,
    new_outer_bitwidth: u32,
    new_inner_bitwidth: u32,
) ->  MigrationResult<Cid> {
    let in_v2_root_node_outer = Map2::load(&root, store, ActorVersion::V2).map_err(|e| MigrationError::HAMTLoad(e.to_string()))?;

    let mut out_v3_root_node_outer = make_empty_map(store, new_outer_bitwidth as u32); // FIXME BIT WIDTH needs to be different based on v2/v3

    in_v2_root_node_outer.for_each(|k: &BytesKey, v: &Cid| {
        let out_inner = migrate_hamt_raw::<_, V>(store, *v, new_inner_bitwidth)?;
        out_v3_root_node_outer.set(k.clone(), out_inner)?;

        Ok(())
    }).map_err(|e| MigrationError::MigrateHAMT(e.to_string()))?;

    let root_cid = out_v3_root_node_outer.flush().map_err(|_| {
        MigrationError::FlushFailed("nv10 migration: hamt hamt flush failed".to_string()).into()
    });

    root_cid
}

// Migrates a HAMT of AMTs from v2 to v3 without re-encoding values.
pub(crate) fn migrate_hamt_amt_raw<
    BS: BlockStore,
    V: Clone + Serialize + PartialEq + DeserializeOwned,
>(
    store: &BS,
    root: Cid,
    new_outer_bitwidth: u32,
    new_inner_bitwidth: u32,
) ->  MigrationResult<Cid> {
    let in_v2_root_node_outer = Map2::load(&root, store, ActorVersion::V2).map_err(|e| MigrationError::AMTLoad(e.to_string()))?;

    let mut out_v3_root_node_outer = make_empty_map(store, new_outer_bitwidth as u32);

    in_v2_root_node_outer.for_each(|k: &BytesKey, v: &Cid| {
        let out_inner = migrate_amt_raw::<_, V>(store, *v, new_inner_bitwidth as i32)?;
        out_v3_root_node_outer.set(k.clone(), out_inner)?;
        Ok(())
    }).map_err(|e| MigrationError::MigrateHAMT(e.to_string()))?;

    let root_cid = out_v3_root_node_outer.flush().map_err(|_| {
        MigrationError::FlushFailed("nv10 migration: hamt amt flush failed".to_string()).into()
    });

    root_cid
}