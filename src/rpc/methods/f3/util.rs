// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use once_cell::sync::Lazy;

const F3_PERMANENT_PARTICIPATING_MINER_IDS_ENV_KEY: &str =
    "FOREST_F3_PERMANENT_PARTICIPATING_MINER_ADDRESSES";

pub static F3_PERMANENT_PARTICIPATING_MINER_IDS: Lazy<Option<HashSet<u64>>> =
    Lazy::new(get_f3_permanent_participating_miner_ids);

/// loads f3 permanent participating miner IDs.
/// Note that this environment variable should only be used for testing purpose.
fn get_f3_permanent_participating_miner_ids() -> Option<HashSet<u64>> {
    if let Ok(permanent_addrs) = std::env::var(F3_PERMANENT_PARTICIPATING_MINER_IDS_ENV_KEY) {
        let mut ids = HashSet::default();
        for addr_str in permanent_addrs.split(",") {
            let Ok(addr) = Address::from_str(addr_str.trim()) else {
                tracing::warn!("Failed to parse miner address {addr_str} set in {F3_PERMANENT_PARTICIPATING_MINER_IDS_ENV_KEY}");
                continue;
            };
            let Ok(id) = addr.id() else {
                tracing::warn!("miner address {addr_str} set in {F3_PERMANENT_PARTICIPATING_MINER_IDS_ENV_KEY} is not an id address");
                continue;
            };
            ids.insert(id);
        }
        if !ids.is_empty() {
            Some(ids)
        } else {
            None
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_f3_permanent_participating_miner_ids() {
        // empty
        std::env::set_var(F3_PERMANENT_PARTICIPATING_MINER_IDS_ENV_KEY, "");
        assert!(get_f3_permanent_participating_miner_ids().is_none());

        // 1 valid address
        std::env::set_var(F3_PERMANENT_PARTICIPATING_MINER_IDS_ENV_KEY, "t01000");
        assert_eq!(
            get_f3_permanent_participating_miner_ids(),
            Some(HashSet::from_iter([1000])),
        );

        // 1 invalid address
        std::env::set_var(F3_PERMANENT_PARTICIPATING_MINER_IDS_ENV_KEY, "tf1000");
        assert!(get_f3_permanent_participating_miner_ids().is_none());

        // 1 bls address
        std::env::set_var(F3_PERMANENT_PARTICIPATING_MINER_IDS_ENV_KEY, "t3sw466j35hqjbch5x7tcr7ona6idsgzypoturfci2ajqsfrrwhp7ty3ythtd7x646adaidnvxpdr5b2ftcciq");
        assert!(get_f3_permanent_participating_miner_ids().is_none());

        // 1 valid address and 1 invalid address with extra whitespaces
        std::env::set_var(
            F3_PERMANENT_PARTICIPATING_MINER_IDS_ENV_KEY,
            "t01000, t3sw466j35hqjbch5x7tcr7ona6idsgzypoturfci2ajqsfrrwhp7ty3ythtd7x646adaidnvxpdr5b2ftcciq, ",
        );
        assert_eq!(
            get_f3_permanent_participating_miner_ids(),
            Some(HashSet::from_iter([1000])),
        );
    }
}
