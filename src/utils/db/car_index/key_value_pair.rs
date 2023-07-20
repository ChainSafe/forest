use super::Hash;
use super::BlockPosition;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyValuePair {
    pub hash: Hash,
    pub value: BlockPosition,
}

impl KeyValuePair {
    // Optimal offset for a hash with a given table length
    pub fn optimal_offset(&self, len: usize) -> usize {
        self.hash.optimal_offset(len)
    }

    // Walking distance between `at` and the optimal location of `hash`
    pub fn distance(&self, at: usize, len: usize) -> usize {
        self.hash.distance(at, len)
    }
}
