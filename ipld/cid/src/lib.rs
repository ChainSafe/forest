use dep_cid::Cid as DepCid;
pub use dep_cid::{Codec, Version};
use std::ops::{Deref, DerefMut};

/// Representation of an IPLD Cid
#[derive(PartialEq, Eq, Clone, Debug, Hash)]
pub struct Cid {
    cid: DepCid,
}

impl From<DepCid> for Cid {
    fn from(cid: DepCid) -> Self {
        Self { cid }
    }
}

impl Default for Cid {
    fn default() -> Self {
        Self {
            cid: DepCid::new(Codec::Raw, Version::V0, &[]),
        }
    }
}

impl Deref for Cid {
    type Target = DepCid;
    fn deref(&self) -> &Self::Target {
        &self.cid
    }
}

impl DerefMut for Cid {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.cid
    }
}

impl Cid {
    /// Cid constructor
    pub fn new(cid: DepCid) -> Self {
        Self { cid }
    }
    /// Constructs a v1 cid with a given codec and bytes
    pub fn from_bytes_v1<B>(codec: Codec, bz: B) -> Self
    where
        B: AsRef<[u8]>,
    {
        Self {
            cid: DepCid::new(codec, Version::V1, bz.as_ref()),
        }
    }
}
