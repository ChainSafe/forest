use std::num::NonZeroU64;

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

/// Desired bucket for a hash with a given table length
pub fn to_bucket(hash: NonMaximalU64, num_buckets: NonZeroU64) -> u64 {
    ((hash.get() as u128 * num_buckets.get() as u128) >> 64) as u64
}

pub fn distance(hash: NonMaximalU64, num_buckets: NonZeroU64, actual_bucket: u64) -> u64 {
    let ideal_bucket = to_bucket(hash, num_buckets);
    if ideal_bucket > actual_bucket {
        num_buckets.get() - ideal_bucket + actual_bucket
    } else {
        actual_bucket - ideal_bucket
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck::quickcheck;

    quickcheck! {
        fn always_in_range(hash: NonMaximalU64, num_buckets: NonZeroU64) -> bool {
            to_bucket(hash, num_buckets) < num_buckets.get()
        }
    }
}
