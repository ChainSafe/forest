// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use thiserror::Error;

mod block;
mod build_chain;
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

#[cfg(test)]
pub(crate) use __chain::chain;

#[cfg(test)]
pub mod __chain {
    use super::*;
    use crate::shim::address::Address;
    use itertools::Itertools as _;

    const AUTO_MINER_ADDRESS_OFFSET: u64 = 100_000;

    /// Implementation detail for [`chain`].
    ///
    /// [`RawBlockHeader::miner_address`]es equal to [`Address::default()`] will be
    /// overwritten with addresses starting from [`AUTO_MINER_ADDRESS_OFFSET`].
    ///
    /// You should NOT rely on this behaviour
    pub fn __chain<const N: usize>(
        parent_tipset: &Tipset,
        mut children: [RawBlockHeader; N],
    ) -> (Tipset, [RawBlockHeader; N]) {
        for (ix, child) in children.iter_mut().enumerate() {
            child.parents = parent_tipset.key().clone();
            child.epoch = parent_tipset.epoch() + 1;
            if child.miner_address == Address::default() {
                child.miner_address =
                    Address::new_id(AUTO_MINER_ADDRESS_OFFSET + u64::try_from(ix).unwrap())
            }
        }

        let tipset = Tipset::new(children.clone()).expect("bug in implementation of chain!{..}"); // or a collision in the auto addresses

        (tipset, children)
    }

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
            crate::blocks::__chain::chain! {
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
                    .map(|it| (it.cid(), fvm_ipld_encoding::to_vec(it).unwrap())),

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
                let $origin_ident = &binding; // create references so users don't have to `.clone()` everywhere
            )*

            let _tipset = Tipset::new(
                [$($origin_ident,)*].into_iter().cloned(),
            )
            .expect("root of `chain!{..}` is not a valid tipset. Either construct a tipset, or use a tipset from a lineage generated by `chain!{..}`");

            // create descendants
            $(
                let (_tipset, binding) = crate::blocks::__chain::__chain(&_tipset, [
                    $({
                        let _ = stringify!($descendant_block); // refer to the repeating variable
                        let _initializer = crate::blocks::RawBlockHeader::default();
                        $( let _initializer = crate::blocks::RawBlockHeader::from($descendant_initializer); )?
                        _initializer
                    }),*
                ]);
                let [ $($descendant_block),* ] = core::array::from_fn(|ix| &binding[ix] );
            )*
        };
    }
    #[cfg(test)]
    pub(crate) use chain;

    #[allow(unused_variables)]
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
                weight: 1.into(),
                ..Default::default()
            }]
        };

        // now define the fork
        chain! {
            [c = c] // bind an existing block header
            -> [e = RawBlockHeader {
                // addresses which are non-default are guaranteed to be preserved
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

    #[test]
    #[allow(unused_variables)]
    fn can_fork_multi() {
        chain! {
            [genesis] -> [a, b] -> [c, d]
        }
        chain! {
            [a = a, b = b]
            -> [
                e = RawBlockHeader {
                    miner_address: Address::new_id(1),
                    ..Default::default()
                },
                f = RawBlockHeader {
                    miner_address: Address::new_id(2),
                    ..Default::default()
                }
            ]
        }

        assert!([c, d, e, f].iter().all_unique());
        let cd = Tipset::new([c, d].into_iter().cloned()).unwrap();
        let ef = Tipset::new([e, f].into_iter().cloned()).unwrap();
        assert_ne!(cd, ef);
        assert_eq!(cd.parents(), ef.parents());
    }
}

#[cfg(any(test, doc))]
mod tests {

    mod serialization_vectors;
    mod ticket_test;
}
