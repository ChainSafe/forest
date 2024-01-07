// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use thiserror::Error;

mod block;
mod election_proof;
mod gossip_block;
mod header;
mod ticket;
#[cfg(not(doc))]
mod tipset;
#[cfg(doc)]
pub mod tipset;
mod vrf_proof;

pub use block::{Block, TxMeta, BLOCK_MESSAGE_LIMIT};
pub use election_proof::ElectionProof;
pub use gossip_block::GossipBlock;
pub use header::{CachingBlockHeader, RawBlockHeader};
pub use ticket::Ticket;
pub use tipset::{FullTipset, Tipset, TipsetKey};
pub use vrf_proof::VRFProof;

/// Blockchain blocks error
#[derive(Debug, PartialEq, Eq, Error)]
pub enum Error {
    /// Tipset contains invalid data, as described by the string parameter.
    #[error("Invalid tipset: {0}")]
    InvalidTipset(String),
    /// The given tipset has no blocks
    #[error("No blocks for tipset")]
    NoBlocks,
    /// Invalid signature
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),
    /// Error in validating arbitrary data
    #[error("Error validating data: {0}")]
    Validation(String),
}

/// Implementation detail of [`chain`]
#[doc(hidden)]
pub fn __chain_tipsets<const N: usize>(
    parent_tipset: &[&RawBlockHeader],
    mut child_tipset: [RawBlockHeader; N],
) -> [RawBlockHeader; N] {
    assert!(
        !parent_tipset.is_empty(),
        "this macro doesn't support null tipsets"
    );

    for parent in parent_tipset {
        assert_eq!(
            parent.epoch, parent_tipset[0].epoch,
            "parent tipset has inconsistent epoch"
        )
    }

    let parents = parent_tipset
        .iter()
        .map(|it| it.cid())
        .collect::<TipsetKeys>();

    for child in &mut child_tipset {
        child.parents = parents.clone();
        child.epoch = parent_tipset[0].epoch + 1;
    }
    child_tipset
}

/// See [`tests::chain_macro_walkthrough`].
macro_rules! chain {
    (
        [ // origin_blockset
            $(
                $origin_ident:ident
                $(= $origin_initializer:expr)?
            ),+ $(,)?
        ] // end origin_blockset
        $(
            ->
            [ // descendant_blockset
                $(
                    $descendant_block:ident
                    $(= $descendant_initializer:expr)?
                ),+ $(,)?
            ] // end descendant_blockset
        )*
    ) => {
        // `_` prefixes are a bootleg `#[allow(unused)]`

        // create each origin block
        $(
            let $origin_ident = {
                let _initializer = RawBlockHeader::default();
                $( let _initializer: RawBlockHeader = $origin_initializer; )?
                _initializer
            };
        )*

        // initialize scratch blockset
        let mut _parents: &[&RawBlockHeader] = &[
            $(&$origin_ident),*
        ];

        // create descendants
        $(
            let [ $($descendant_block),* ] = __chain_tipsets(_parents, [
                $({
                    let _ = stringify!($descendant_block); // refer to the repeating variable
                    let _initializer = RawBlockHeader::default();
                    $( let _initializer: RawBlockHeader = $descendant_initializer; )?
                    _initializer
                }),*
            ]);
            let binding = [
                $(&$descendant_block),*
            ];
            _parents = &binding;
        )*
    };
}
pub(crate) use chain;

#[cfg(any(test, doc))]
mod tests {
    use crate::shim::address::Address;

    use super::*;

    mod serialization_vectors;
    mod ticket_test;

    #[test]
    fn chain_macro_walkthrough() {
        // we will create the following forked chain:
        //
        // [genesis] -> [a, b] -> [c] -> [d]
        //                            -> [e] -> [f]

        chain! {
            // specify a tipset of block headers in square brackets
            // a new (default) header will be created for each variable name
            [genesis]
            -> [a, b] // chain on a child tipset.
                      // the epoch and parents will be incremented as appropriate
            -> [c]
            -> [d = RawBlockHeader {
                // you can also specify fields
                miner_address: Address::new_id(1),
                ..Default::default()
            }]
        };

        // now define the fork
        chain! {
            [c = c] // bind an existing block header
            -> [e = RawBlockHeader {
                miner_address: Address::new_id(2),
                ..Default::default()
            }]
            -> [f]
        };

        assert_eq!(genesis.epoch, 0);
        assert_eq!(a.epoch, 1);
        assert_eq!(
            a.parents.cids.into_iter().collect::<Vec<_>>(),
            vec![genesis.cid()]
        );

        assert_ne!(d, e); // these two fork
        assert_eq!(d.epoch, e.epoch);
        assert_eq!(d.parents, e.parents);
    }
}
