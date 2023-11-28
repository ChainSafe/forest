/// Like [`std::num::NonZeroU64`], but is never [`u64::MAX`]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct NonMaximalU64(u64);

impl NonMaximalU64 {
    pub fn new(u: u64) -> Option<Self> {
        match u == u64::MAX {
            true => None,
            false => Some(Self(u)),
        }
    }
    pub fn fit(u: u64) -> Self {
        Self(u.saturating_sub(1))
    }
    pub fn get(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
impl quickcheck::Arbitrary for NonMaximalU64 {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        Self::fit(u64::arbitrary(g))
    }
}
