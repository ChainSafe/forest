// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    beacon::BeaconEntry,
    blocks::*,
    shim::{
        address::Address, clock::ChainEpoch, crypto::Signature, econ::TokenAmount,
        sector::PoStProof,
    },
};
use cid::Cid;
use either::Either;
use itertools::Itertools as _;
use num_bigint::BigInt;
use petgraph::{
    visit::{IntoNeighborsDirected, Walker},
    Direction,
};
use sealed::Override;
use std::{fmt::Debug, hash::Hash};

mod sealed {

    /// This struct is sealed - you may not directly construct one.
    #[derive(Default, Debug, Clone)]
    pub enum Override<T> {
        #[default]
        Open,
        Closed(T),
        Overridden(T),
    }

    impl<T> From<T> for Override<T> {
        fn from(value: T) -> Self {
            Self::Overridden(value)
        }
    }

    impl<T> Override<T> {
        pub fn insert_or_panic(&mut self, it: T)
        where
            T: PartialEq,
        {
            match self {
                Override::Open => *self = Override::Closed(it),
                Override::Closed(already) | Override::Overridden(already) => match already == &it {
                    true => {}
                    false => panic!("incompatible value in `Override`"),
                },
            }
        }
        pub fn into_fixed(self) -> Option<T> {
            match self {
                Override::Open => None,
                Override::Closed(it) | Override::Overridden(it) => Some(it),
            }
        }
        pub fn fixed(&self) -> Option<&T> {
            match self {
                Override::Open => None,
                Override::Closed(it) | Override::Overridden(it) => Some(it),
            }
        }
        pub fn close_with(&mut self, with: impl FnOnce() -> T) {
            match self {
                Override::Open => *self = Override::Closed(with()),
                Override::Closed(_) | Override::Overridden(_) => {}
            }
        }
        pub fn could_be(&self, it: &T) -> bool
        where
            T: PartialEq,
        {
            match self {
                Override::Open => true,
                Override::Closed(already) | Override::Overridden(already) => already == it,
            }
        }
    }
}

#[derive(Default, Debug)]
struct BlockHeaderGraph<'a> {
    ident2spec: ahash::HashMap<&'a str, HeaderBuilder>,
    hierarchy: petgraph::graphmap::DiGraphMap<&'a [&'a str], ()>,
}

impl BlockHeaderGraph<'_> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn solve(&self) -> ahash::HashMap<&str, RawBlockHeader> {
        let ccs = petgraph::algo::connected_components(&self.hierarchy);
        assert_eq!(
            ccs, 1,
            "only chains with a single root tipset are supported"
        );
        assert!(!petgraph::algo::is_cyclic_directed(&self.hierarchy));

        let mut root_tipset_idents = None;
        for tipset in self.hierarchy.nodes() {
            assert!(!tipset.is_empty(), "null tipsets are not supported");
            match self
                .hierarchy
                .neighbors_directed(tipset, Direction::Incoming)
                .count()
            {
                0 => {
                    assert!(
                        root_tipset_idents.is_none(),
                        "tree must have exactly one root. Duplicate: {:?}",
                        tipset
                    );
                    root_tipset_idents = Some(tipset);
                }
                1 => {}
                _ => panic!("tipset hierarchy is not a tree"),
            }
        }
        let root_tipset_idents = root_tipset_idents.expect("tipset has no root");

        let mut solved = ahash::HashMap::<&str, RawBlockHeader>::default();
        for parent_tipset_idents in
            petgraph::algo::toposort(&self.hierarchy, None).expect("tipset hierarchy is not a tree")
        {
            if parent_tipset_idents == root_tipset_idents {
                let root = root_tipset_idents
                    .iter()
                    .map(|it| self.ident2spec.get_key_value(it).unwrap());

                let epoch = get_overridden(root.clone().map(|(_, it)| &it.epoch), "epoch")
                    .copied()
                    .unwrap_or_default();
                let parents = get_overridden(root.clone().map(|(_, it)| &it.parents), "parents")
                    .cloned()
                    .unwrap_or_default();
                let state_root =
                    get_overridden(root.clone().map(|(_, it)| &it.state_root), "state_root")
                        .copied()
                        .unwrap_or_default();

                let mut filler_addresses = get_filler_addresses(
                    root.clone()
                        .filter_map(|(_, it)| it.miner_address.fixed())
                        .cloned(),
                );

                for (root_ident, root_spec) in root {
                    let mut block = root_spec.clone();
                    block.epoch.insert_or_panic(epoch);
                    block.parents.insert_or_panic(parents.clone());
                    block.state_root.insert_or_panic(state_root);
                    block
                        .miner_address
                        .close_with(|| filler_addresses.next().unwrap());
                    let clobbered =
                        solved.insert(root_ident, block.build(RawBlockHeader::default()));
                    assert!(clobbered.is_none());
                }
            }

            let parent_tipset =
                Tipset::new(parent_tipset_idents.iter().map(|it| &solved[it]).cloned()).expect(
                    "internal error in BlockHeaderGraph solving - this is a bug in the solver code",
                );

            let children = self
                .hierarchy
                .neighbors_directed(parent_tipset_idents, Direction::Outgoing)
                .flat_map(|idents| idents.iter().map(|ident| (ident, &self.ident2spec[ident])));

            let mut filler_addresses = get_filler_addresses(
                children
                    .clone()
                    .filter_map(|(_, it)| it.miner_address.fixed())
                    .cloned(),
            );

            for (child_ident, child_spec) in children {
                let mut child = child_spec.clone();

                child.epoch.insert_or_panic(parent_tipset.epoch() + 1);
                child.state_root.insert_or_panic(Cid::default());
                child.parents.insert_or_panic(parent_tipset.key().clone());
                child
                    .miner_address
                    .close_with(|| filler_addresses.next().unwrap());

                let clobbered = solved.insert(child_ident, child.build(RawBlockHeader::default()));
                assert!(clobbered.is_none());
            }
        }
        solved
    }
}

fn get_filler_addresses(
    excluding: impl IntoIterator<Item = Address>,
) -> impl Iterator<Item = Address> {
    let excluding = excluding.into_iter().collect::<Vec<_>>();
    assert!(
        excluding.iter().all_unique(),
        "Duplicate override miner addresses"
    );

    (0..)
        .map(Address::new_id)
        .filter(move |it| !excluding.contains(it))
}

fn get_overridden<'a, T: Clone + Eq + Hash + Debug + 'a>(
    items: impl IntoIterator<Item = &'a Override<T>>,
    field_name: &str,
) -> Option<&'a T> {
    match items
        .into_iter()
        .filter_map(|it| it.fixed())
        .unique()
        .at_most_one()
    {
        Ok(one) => one,
        Err(e) => panic!(
            "Multiple different overrides found for field {field_name}: {:?}",
            e.collect::<Vec<_>>()
        ),
    }
}

fn solve_root_tipset(tipset: &mut [&mut HeaderBuilder]) {
    fn propogate<'a, T: Clone + Eq + Hash + Debug + 'a>(
        items: impl IntoIterator<Item = &'a mut Override<T>>,
        default: T,
        field_name: &str,
    ) {
        let items = items.into_iter().collect::<Vec<_>>();
        let value = match items
            .iter()
            .filter_map(|it| it.fixed())
            .unique()
            .at_most_one()
        {
            Ok(one) => one.cloned().unwrap_or(default),
            Err(e) => panic!(
                "Multiple different overrides found for field {field_name}: {:?}",
                e.collect::<Vec<_>>()
            ),
        };
        for item in items {
            *item = Override::Closed(value.clone())
        }
    }

    assert!(
        !tipset.is_empty(),
        "tipsets may not be empty (null tipsets are not supported)"
    );
    propogate(tipset.iter_mut().map(|it| &mut it.epoch), 0, "epoch");
    propogate(
        tipset.iter_mut().map(|it| &mut it.parents),
        TipsetKey::default(),
        "parents",
    );
    propogate(
        tipset.iter_mut().map(|it| &mut it.state_root),
        Cid::default(),
        "state_root",
    );
    let fixed_miner_addresses = tipset.iter().filter_map(|it| it.miner_address.fixed());
    assert!(
        fixed_miner_addresses.clone().all_unique(),
        "Duplicate override miner addresses"
    );
    let taken_miner_addresses = fixed_miner_addresses
        .cloned()
        .collect::<ahash::HashSet<_>>();
    let mut new_miner_addresses = (0..)
        .map(Address::new_id)
        .filter(|it| !taken_miner_addresses.contains(it));
    for header in tipset {
        header
            .miner_address
            .close_with(|| new_miner_addresses.next().unwrap())
    }
}

#[derive(Default, Debug, Clone)]
pub struct HeaderBuilder {
    pub miner_address: Override<Address>,
    pub ticket: Override<Option<Ticket>>,
    pub election_proof: Override<Option<ElectionProof>>,
    pub beacon_entries: Override<Vec<BeaconEntry>>,
    pub winning_post_proof: Override<Vec<PoStProof>>,
    pub parents: Override<TipsetKey>,
    pub weight: Override<BigInt>,
    pub epoch: Override<ChainEpoch>,
    pub state_root: Override<Cid>,
    pub message_receipts: Override<Cid>,
    pub messages: Override<Cid>,
    pub bls_aggregate: Override<Option<Signature>>,
    pub timestamp: Override<u64>,
    pub signature: Override<Option<Signature>>,
    pub fork_signal: Override<u64>,
    pub parent_base_fee: Override<TokenAmount>,
}

impl HeaderBuilder {
    pub fn new() -> Self {
        Self::default()
    }
}

macro_rules! setters {
    ($($setter_name:ident -> $field_name:ident: $field_ty:ty);* $(;)?) => {
        $(
            pub fn $setter_name(&mut self, it: $field_ty) -> &mut Self {
                self.$field_name = Override::Overridden(it);
                self
            }
        )*
    }
}

#[allow(unused)]
impl HeaderBuilder {
    setters! {
        with_miner_address -> miner_address: Address;
        with_ticket -> ticket: Option<Ticket>;
        with_election_proof -> election_proof: Option<ElectionProof>;
        with_beacon_entries -> beacon_entries: Vec<BeaconEntry>;
        with_winning_post_proof -> winning_post_proof: Vec<PoStProof>;
        with_parents -> parents: TipsetKey;
        with_weight -> weight: BigInt;
        with_epoch -> epoch: ChainEpoch;
        with_state_root -> state_root: Cid;
        with_message_receipts -> message_receipts: Cid;
        with_messages -> messages: Cid;
        with_bls_aggregate -> bls_aggregate: Option<Signature>;
        with_timestamp -> timestamp: u64;
        with_signature -> signature: Option<Signature>;
        with_fork_signal -> fork_signal: u64;
        with_parent_base_fee -> parent_base_fee: TokenAmount;
    }
}

impl HeaderBuilder {
    fn build(self, fill: RawBlockHeader) -> RawBlockHeader {
        macro_rules! fill {
            ($($ident:ident),* $(,)?) => {
                RawBlockHeader {
                    $(
                        $ident: self.$ident.into_fixed().unwrap_or(fill.$ident),
                    )*
                }
            }
        }
        fill! {
            miner_address,
            ticket,
            election_proof,
            beacon_entries,
            winning_post_proof,
            parents,
            weight,
            epoch,
            state_root,
            message_receipts,
            messages,
            bls_aggregate,
            timestamp,
            signature,
            fork_signal,
            parent_base_fee,
        }
    }
}

impl From<RawBlockHeader> for HeaderBuilder {
    fn from(value: RawBlockHeader) -> Self {
        macro_rules! overriden {
            ($($ident:ident),* $(,)?) => {
                let RawBlockHeader {
                    $($ident,)*
                } = value;
                Self {
                    $($ident: Override::Overridden($ident),)*
                }
            };
        }
        overriden! {
            miner_address,
            ticket,
            election_proof,
            beacon_entries,
            winning_post_proof,
            parents,
            weight,
            epoch,
            state_root,
            message_receipts,
            messages,
            bls_aggregate,
            timestamp,
            signature,
            fork_signal,
            parent_base_fee,
        }
    }
}
