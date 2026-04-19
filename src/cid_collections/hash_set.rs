// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use bytes::Bytes;
use cid::Cid;

#[cfg(doc)]
use std::collections::HashSet;
use std::sync::LazyLock;

pub trait CidHashSetLike {
    /// Adds a value to the set.
    ///
    /// Returns whether the value was newly inserted.
    fn insert(&mut self, cid: Cid) -> bool;
}

/// A hash set implemented as a `HashMap` where the value is `()`.
///
/// See also [`HashSet`].
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct CidHashSet {
    inner: CidHashMap<()>,
}

impl CidHashSet {
    /// Creates an empty `HashSet`.
    ///
    /// See also [`HashSet::new`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a value to the set.
    ///
    /// Returns whether the value was newly inserted.
    ///
    /// See also [`HashSet::insert`].
    pub fn insert(&mut self, cid: Cid) -> bool {
        self.inner.insert(cid, ()).is_none()
    }

    /// Returns the number of elements.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if the set contains a `Cid`.
    #[allow(dead_code)]
    pub fn contains(&self, cid: &Cid) -> bool {
        self.inner.contains_key(cid)
    }

    /// Removes a `Cid` from the set. Returns whether the value was present in the set.
    #[allow(dead_code)]
    pub fn remove(&mut self, cid: &Cid) -> bool {
        self.inner.remove(cid).is_some()
    }

    /// Returns `true` if the set is empty.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl CidHashSetLike for CidHashSet {
    fn insert(&mut self, cid: Cid) -> bool {
        self.insert(cid)
    }
}

////////////////////
// Collection Ops //
////////////////////

impl Extend<Cid> for CidHashSet {
    fn extend<T: IntoIterator<Item = Cid>>(&mut self, iter: T) {
        self.inner.extend(iter.into_iter().map(|it| (it, ())))
    }
}

impl FromIterator<Cid> for CidHashSet {
    fn from_iter<T: IntoIterator<Item = Cid>>(iter: T) -> Self {
        let mut this = Self::new();
        this.extend(iter);
        this
    }
}

/// A file-backed CID hash set.
/// This is intended to be used for large sets of CIDs that may not fit in memory, such as when tracking seen CIDs during a chain export.
pub struct FileBackedCidHashSet {
    db: parity_db::Db,
    // for dropping the temporary directory when the set is dropped
    _dir: tempfile::TempDir,
    lru: hashlink::LruCache<SmallCid, ()>,
}

impl Default for FileBackedCidHashSet {
    fn default() -> Self {
        const MAX_ATTEMPTS: usize = 10;
        for _ in 0..MAX_ATTEMPTS {
            // temp dir has limitation of 10GiB on some platforms, so we prefer current working directory
            // and try multiple times to create one in case we hit that limit
            if let Some(dir) = tempfile::tempdir_in(".")
                .ok()
                .or_else(|| tempfile::tempdir().ok())
            {
                let options = parity_db::Options {
                    path: dir.path().to_path_buf(),
                    sync_wal: false,
                    sync_data: false,
                    stats: false,
                    salt: None,
                    columns: vec![
                        parity_db::ColumnOptions {
                            uniform: true,
                            append_only: true,
                            ..Default::default()
                        },
                        parity_db::ColumnOptions {
                            append_only: true,
                            ..Default::default()
                        },
                    ],
                    compression_threshold: Default::default(),
                };
                if let Ok(db) = parity_db::Db::open_or_create(&options) {
                    return Self {
                        db,
                        _dir: dir,
                        #[allow(clippy::disallowed_methods)]
                        lru: hashlink::LruCache::new(2 << 19), // ~80MiB for 1M entries
                    };
                }
            }
        }
        panic!(
            "failed to create parity db with a temporary directory after {MAX_ATTEMPTS} attempts"
        );
    }
}

impl CidHashSetLike for FileBackedCidHashSet {
    fn insert(&mut self, cid: Cid) -> bool {
        static EMPTY_VALUE: LazyLock<Bytes> = LazyLock::new(|| Bytes::from_static(&[]));

        let small = SmallCid::from(cid);
        if self.lru.get(&small).is_some() {
            return false;
        }

        let (col, key) = match &small {
            SmallCid::Inline(c) => (0, c.digest().to_vec()),
            SmallCid::Indirect(u) => (1, u.inner().to_bytes()),
        };
        if self.db.get(col, &key).ok().flatten().is_some() {
            self.lru.insert(small, ());
            false
        } else {
            _ = self
                .db
                .commit_changes_bytes([(col, parity_db::Operation::Set(key, EMPTY_VALUE.clone()))]);
            self.lru.insert(small, ());
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[quickcheck_macros::quickcheck]
    fn test_cid_hashset(mut cids: Vec<Cid>) {
        cids.dedup();
        let mut set = CidHashSet::default();
        for cid in cids.iter() {
            all_asserts::assert_true!(set.insert(*cid), "expected CID to be newly inserted");
        }
        for cid in cids.iter() {
            all_asserts::assert_false!(set.insert(*cid), "expected CID to be present in the set");
        }
    }

    #[quickcheck_macros::quickcheck]
    fn test_file_backed_cid_hashset(mut cids: Vec<Cid>) {
        cids.dedup();
        let mut set = FileBackedCidHashSet::default();
        let dir = set._dir.path().to_path_buf();
        for cid in cids.iter() {
            all_asserts::assert_true!(set.insert(*cid), "expected CID to be newly inserted");
        }
        for cid in cids.iter() {
            all_asserts::assert_false!(set.insert(*cid), "expected CID to be present in the set");
        }
        drop(set);
        all_asserts::assert_false!(dir.exists(), "expected temporary directory to be deleted");
    }
}
