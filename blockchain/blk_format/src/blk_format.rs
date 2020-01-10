// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use cid::{Cid, Codec, Version};
use std::fmt;
use multihash::{Hash, encode};

/// Block provides abstraction for blocks implementations.
trait Block {
    fn raw_data(&self) -> Vec<u8>;
    fn cid(&self) -> Cid;
    fn multihash(&self) -> Hash;
}
/// A BasicBlock is a singular block of data in ipfs format. It implements the Block
/// trait.
struct BasicBlock {
    cid: Cid,
    data: Vec<u8>,
}

impl BasicBlock {
    /// NewBlock creates a Block object from opaque data. It will hash the data.
    fn new(&self, data: Vec<u8>) -> Self {
        // TODO
        // replace SHA2256 with Blake2b when rust-cid updates multihash dependency
        let h = encode(Hash::SHA2256, data).unwrap();
        return Self {
            cid: Cid::new(Codec::DagCBOR, Version::V1, &h),
            data,
        }
    }
    /// NewBlockWithCid creates a new block when the hash of the data
    /// is already known.
    fn new_blk_with_cid(&self, data: Vec<u8>, cid: Cid) -> Self {
        // TODO
        // check cryptographic sum of buffer; see https://github.com/ipfs/go-block-format/blob/master/blocks.go#L45
        // cid library does not have Sum method
        // go version https://github.com/multiformats/go-multihash/blob/master/sum.go#L36
        return Self {
            cid,
            data
        }
    }
}

impl Block for BasicBlock {
    /// returns the block raw contents as a byte array
    fn raw_data(&self) -> Vec<u8> {
        self.data
    }
    /// returns the content identifier of the block
    fn cid(&self) -> Cid {
        self.cid
    }
    /// returns the hash contained in the block CID
    fn multihash(&self) -> Hash {
        self.cid.hash
    }
}

/// human-readable string representation of a block CID
impl fmt::Display for BasicBlock {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "block: {}",
            self.cid
        )
    }
}