// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use itertools::Itertools as _;
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
pub use tipset::{CreateTipsetError, FullTipset, Tipset, TipsetKey};
pub use vrf_proof::VRFProof;

/// Blockchain blocks error
#[derive(Debug, PartialEq, Eq, Error)]
pub enum Error {
    /// Invalid signature
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),
    /// Error in validating arbitrary data
    #[error("Error validating data: {0}")]
    Validation(String),
}

/// [`RawBlockHeader::miner_address`]es equal to [`Address::default()`] will be
/// overwritten with unique addresses
fn __tipsetify<const N: usize>(
    parent_cids: TipsetKey,
    child_epoch: i64,
    mut children: [RawBlockHeader; N],
) -> (TipsetKey, i64, [RawBlockHeader; N]) {
    // catch a footgun
    let mut non_default_addresses = children.iter().flat_map(|it| {
        match it.miner_address == RawBlockHeader::default().miner_address {
            true => None,
            false => Some(it.miner_address),
        }
    });

    // Tipsets must have unique `miner_address`es
    // This macro will only overwrite miner_addresses which already have the default value
    assert!(non_default_addresses.all_unique());

    let mut observed_addresses = ahash::HashSet::default();
    // users are more likely to pick small numbers for their tests, so count down from the end
    let mut next_auto_address_id = 0;

    for child in &mut children {
        child.parents = parent_cids.clone();
        child.epoch = child_epoch;
        if child.miner_address == RawBlockHeader::default().miner_address {
            // GOTCHA: this behaviour could surprise users if they specify an address and it's silently overridden
            while {
                let is_new = observed_addresses.insert(child.miner_address);
                !is_new
            } {
                child.miner_address = Address::new_id(next_auto_address_id);
                next_auto_address_id += 1;
            }
        }
    }

    let tipset = Tipset::new(
        children
            .clone()
            .into_iter()
            .map(CachingBlockHeader::new)
            .collect(),
    )
    .expect("bug in implementation of chain!{..}");

    (tipset.key().clone(), child_epoch + 1, children)
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
        .collect::<TipsetKey>();

    for child in &mut child_tipset {
        child.parents = parents.clone();
        child.epoch = parent_tipset[0].epoch + 1;
    }
    child_tipset
}

#[cfg(test)]
/// See [`tests::chain_macro_walkthrough`].
macro_rules! chain {
    (
        in $blockstore:expr =>
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
        // passthrough
        crate::blocks::chain! {
            [
                $(
                    $origin_ident
                    $(= $origin_initializer)?
                ),+
            ]
            $(
                ->
                [
                    $(
                        $descendant_block
                        $(= $descendant_initializer)?
                    ),+
                ]
            )*
        }
        // insert
        fvm_ipld_blockstore::Blockstore::put_many_keyed(
            $blockstore,
            [
                $(&$origin_ident,)*
                $($(&$descendant_block,)*)*
            ]
                .iter()
                .map(|it| (it.cid(), fvm_ipld_encoding::to_vec(&it).unwrap())),

        ).unwrap()
    };
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
            let binding: crate::blocks::RawBlockHeader = {
                let _initializer = crate::blocks::RawBlockHeader::default();
                // we `.clone()` to allow origin_initializer to be a reference
                // (e.g from a previous invocation of chain!{..}) or a new literal
                // block
                $( let _initializer = crate::blocks::RawBlockHeader::from($origin_initializer.clone()); )?
                _initializer
            };
            let $origin_ident = &binding;
        )*

        // initialize scratch blockset
        let mut _parents: &[&crate::blocks::RawBlockHeader] = &[
            $($origin_ident),*
        ];

        // create descendants
        $(
            let binding = crate::blocks::__chain_tipsets(_parents, [
                $({
                    let _ = stringify!($descendant_block); // refer to the repeating variable
                    let _initializer = crate::blocks::RawBlockHeader::default();
                    $( let _initializer = crate::blocks::RawBlockHeader::from($descendant_initializer); )?
                    _initializer
                }),*
            ]);
            let [ $($descendant_block),* ] = core::array::from_fn(|ix| &binding[ix] );
            let binding = &[ $($descendant_block),* ];
            _parents = binding;
        )*
    };
}
#[cfg(test)]
pub(crate) use chain;

use crate::shim::address::Address;

#[cfg(any(test, doc))]
mod tests {
    use super::*;
    use crate::shim::address::Address;

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
            a.parents.cids.clone().into_iter().collect::<Vec<_>>(),
            vec![genesis.cid()]
        );

        assert_ne!(d, e); // these two fork
        assert_eq!(d.epoch, e.epoch);
        assert_eq!(d.parents, e.parents);
    }

    mod serialization_vectors;
    mod ticket_test;
}
