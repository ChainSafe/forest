use std::num::NonZeroUsize;

use super::NonMaximalU64;
use cid::Cid;

pub fn of(cid: &Cid) -> NonMaximalU64 {
    NonMaximalU64::fit(
        cid.hash()
            .digest()
            .chunks_exact(8)
            .map(<[u8; 8]>::try_from)
            .filter_map(Result::ok)
            .fold(cid.codec() ^ cid.hash().code(), |hash, chunk| {
                hash ^ u64::from_le_bytes(chunk)
            }),
    )
}

/// Desired slot for a hash with a given table length
pub fn ideal_slot_ix(hash: NonMaximalU64, num_buckets: NonZeroUsize) -> usize {
    usize::try_from((hash.get() as u128 * num_buckets.get() as u128) >> 64).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck::quickcheck;

    quickcheck! {
        fn always_in_range(hash: NonMaximalU64, num_buckets: NonZeroUsize) -> bool {
            ideal_slot_ix(hash, num_buckets) < num_buckets.get()
        }
    }
}
