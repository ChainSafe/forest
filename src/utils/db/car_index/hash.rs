use cid::Cid;
use std::ops::Not;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hash(pub u64);

impl Not for Hash {
    type Output = Hash;
    fn not(self) -> Hash {
        Hash(self.0.not())
    }
}

impl From<Hash> for u64 {
    fn from(Hash(hash): Hash) -> u64 {
        hash
    }
}

impl From<u64> for Hash {
    fn from(hash: u64) -> Hash {
        Hash(hash)
    }
}

impl From<Cid> for Hash {
    fn from(cid: Cid) -> Hash {
        Hash::from_le_bytes(cid.hash().digest()[0..8].try_into().unwrap_or([0xFF; 8]))
    }
}

impl Hash {
    const MAX: Hash = Hash(u64::MAX);

    pub fn from_le_bytes(bytes: [u8; 8]) -> Hash {
        Hash(u64::from_le_bytes(bytes))
    }

    // Optimal offset for a hash with a given table length
    pub fn optimal_offset(&self, len: usize) -> usize {
        self.0 as usize % len
    }

    // Walking distance between `at` and the optimal location of `hash`
    pub fn distance(&self, at: usize, len: usize) -> usize {
        let pos = self.optimal_offset(len);
        if pos > at {
            (len - pos + at) % len
        } else {
            (at - pos) % len
        }
    }
}
