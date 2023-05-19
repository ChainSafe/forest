// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_hamt::Hamt;

/// Creates and stores a new empty map, returning its CID.
/// <https://github.com/filecoin-project/go-state-types/blob/master/builtin/v9/util/adt/map.go#L74>
pub fn store_empty_map(store: &impl Blockstore, bitwidth: u32) -> anyhow::Result<Cid> {
    // Value type does not matter here
    let mut map = Hamt::<_, Cid>::new_with_bit_width(store, bitwidth);
    Ok(map.flush()?)
}

#[cfg(test)]
mod tests {
    use fvm_ipld_blockstore::MemoryBlockstore;

    use super::*;

    #[test]
    fn test_store_empty_map() -> anyhow::Result<()> {
        let store = MemoryBlockstore::new();
        let bitwidth = 5;

        let empty_id = store_empty_map(&store, bitwidth)?;

        // Parity test in `tests/go/store_empty_map_test.go`
        anyhow::ensure!(
            empty_id.to_string().as_str()
                == "bafy2bzaceamp42wmmgr2g2ymg46euououzfyck7szknvfacqscohrvaikwfay"
        );

        let empty_id_2 = {
            let mut map = Hamt::<_, String>::new_with_bit_width(&store, bitwidth);
            map.flush()
        }?;

        anyhow::ensure!(empty_id == empty_id_2);

        Ok(())
    }
}
