// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use clock::ChainEpoch;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::convert::TryInto;
use std::ops::Deref;

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub struct OptionalEpoch(pub Option<ChainEpoch>);

impl Deref for OptionalEpoch {
    type Target = Option<ChainEpoch>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Serialize for OptionalEpoch {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.0 {
            Some(epoch) => epoch.serialize(serializer),
            None => (-1i8).serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for OptionalEpoch {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let epoch: i64 = Deserialize::deserialize(deserializer)?;
        Ok(Self(epoch.try_into().ok()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use encoding::{from_slice, to_vec};
    #[test]
    fn optional_serialize() {
        let ep = OptionalEpoch(None);
        let bz = to_vec(&ep).unwrap();
        assert_eq!(from_slice::<OptionalEpoch>(&bz).unwrap(), ep);

        let ep = OptionalEpoch(Some(0));
        let bz = to_vec(&ep).unwrap();
        assert_eq!(from_slice::<OptionalEpoch>(&bz).unwrap(), ep);
    }
}
