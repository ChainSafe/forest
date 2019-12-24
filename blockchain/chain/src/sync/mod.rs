use blocks::Tipset;
use std::ops::{Deref, DerefMut};

/// SyncBucket defines a bucket of tipsets to sync
struct SyncBucket<'a> {
    tips: Vec<&'a Tipset>,
}

impl<'a> Deref for SyncBucket<'a> {
    type Target = Vec<&'a Tipset>;
    fn deref(&self) -> &Self::Target {
        &self.tips
    }
}

impl<'a> DerefMut for SyncBucket<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.tips
    }
}

impl<'a> SyncBucket<'a> {
    /// Constructor for tipset bucket
    fn _new(tips: Vec<&'a Tipset>) -> SyncBucket {
        Self { tips }
    }
    /// heaviest_tipset returns the tipset with the max weight
    fn _heaviest_tipset(&self) -> Option<&'a Tipset> {
        if self.is_empty() {
            return None;
        }

        // return max value pointer
        self.iter().max_by_key(|a| a.weight()).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use address::Address;
    use blocks::{BlockHeader, TipSetKeys};

    fn header_with_weight(weight: u64) -> BlockHeader {
        BlockHeader::builder()
            .parents(TipSetKeys::default())
            .miner_address(Address::new_id(0).unwrap())
            .bls_aggregate(vec![])
            .weight(weight)
            .build()
            .unwrap()
    }

    #[test]
    fn base_constructor() {
        SyncBucket::_new(Vec::new());
    }

    #[test]
    fn heaviest_tipset() {
        let l_tip = Tipset::new(vec![header_with_weight(1)]).unwrap();
        let h_tip = Tipset::new(vec![header_with_weight(2)]).unwrap();

        // Test the comparison of tipsets
        let bucket = SyncBucket::_new(vec![&l_tip, &h_tip]);
        assert_eq!(bucket._heaviest_tipset().unwrap().weight(), 2);

        // assert bucket with just one tipset still resolves
        let bucket = SyncBucket::_new(vec![&l_tip]);
        assert_eq!(bucket._heaviest_tipset().unwrap().weight(), 1);
    }
}
