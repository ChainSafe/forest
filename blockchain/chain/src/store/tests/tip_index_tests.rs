// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod tip_index;
use tip_index::*;
use test_utils::{construct_tipset_metadata};

    #[test]
    fn put_test() {
        let meta = construct_tipset_metadata();
        let mut tip = TipIndex::new();
        assert!(tip.put(&meta).is_ok(), "error setting tip index hash map")
    }

    #[test]
    fn get_from_hashmap() {
        let meta = construct_tipset_metadata();
        let mut tip = TipIndex::new();
        tip.put(&meta).unwrap();
        let mut hasher = DefaultHasher::new();
        meta.tipset.parents().hash::<DefaultHasher>(&mut hasher);
        let result = tip.get(hasher.finish()).unwrap();
        assert_eq!(result, meta);
    }

    #[test]
    fn get_tipset_by_parents() {
        let meta = construct_tipset_metadata();
        let mut tip = TipIndex::new();
        tip.put(&meta).unwrap();
        let result = tip.get_tipset(meta.tipset.parents()).unwrap();
        assert_eq!(result, meta.tipset);
    }

    #[test]
    fn get_state_root_by_parents() {
        let meta = construct_tipset_metadata();
        let mut tip = TipIndex::new();
        tip.put(&meta).unwrap();
        let result = tip.get_tipset_receipts_root(meta.tipset.parents()).unwrap();
        assert_eq!(result, meta.tipset_state_root);
    }

    #[test]
    fn get_receipts_root_by_parents() {
        let meta = construct_tipset_metadata();
        let mut tip = TipIndex::new();
        tip.put(&meta).unwrap();
        let result = tip.get_tipset_receipts_root(meta.tipset.parents()).unwrap();
        assert_eq!(result, meta.tipset_receipts_root);
    }

    #[test]
    fn get_tipset_by_epoch() {
        let meta = construct_tipset_metadata();
        let mut tip = TipIndex::new();
        tip.put(&meta).unwrap();
        let result = tip.get_tipset(&meta.tipset.epoch().clone()).unwrap();
        assert_eq!(result, meta.tipset);
    }

    #[test]
    fn get_state_root_by_epoch() {
        let meta = construct_tipset_metadata();
        let mut tip = TipIndex::new();
        tip.put(&meta).unwrap();
        let result = tip
            .get_tipset_state_root(&meta.tipset.epoch().clone())
            .unwrap();
        assert_eq!(result, meta.tipset_state_root);
    }

    #[test]
    fn get_receipts_root_by_epoch() {
        let meta = construct_tipset_metadata();
        let mut tip = TipIndex::new();
        tip.put(&meta).unwrap();
        let result = tip
            .get_tipset_receipts_root(&meta.tipset.epoch().clone())
            .unwrap();
        assert_eq!(result, meta.tipset_receipts_root);
    }