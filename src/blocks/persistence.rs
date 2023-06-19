// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::utils::db::file_backed_obj::FileBackedObject;
use fvm_ipld_encoding::Cbor;

use crate::blocks::*;

impl FileBackedObject for TipsetKeys {
    fn serialize(&self) -> anyhow::Result<Vec<u8>> {
        Ok(self.marshal_cbor()?)
    }

    fn deserialize(bytes: &[u8]) -> anyhow::Result<Self> {
        Ok(fvm_ipld_encoding::from_slice(bytes)?)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::utils::db::file_backed_obj::FileBacked;
    use anyhow::*;

    use super::*;

    #[test]
    fn tipset_keys_round_trip() -> Result<()> {
        let path = Path::new("src/blocks/tests/calibnet/HEAD");
        let obj1: FileBacked<TipsetKeys> =
            FileBacked::load_from_file_or_create(path.into(), Default::default, None)?;
        let serialized = obj1.inner().serialize()?;
        let deserialized = TipsetKeys::deserialize(&serialized)?;

        ensure!(obj1.inner() == &deserialized);

        Ok(())
    }
}
