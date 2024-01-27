// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    beacon::BeaconEntry,
    blocks::*,
    db::MemoryDB,
    shim::{
        address::Address, clock::ChainEpoch, crypto::Signature, econ::TokenAmount,
        sector::PoStProof,
    },
};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use itertools::Itertools as _;
use num_bigint::BigInt;
use petgraph::Direction;
use sealed::Override;
use std::ops::Index;
use std::{borrow::Borrow, fmt::Debug, hash::Hash};
use std::{
    collections::hash_map::Entry::{Occupied, Vacant},
    iter,
};

#[test]
fn chain4u() {
    let mut c4u = Chain4U::new();
    c4u.insert([], "gen", HeaderBuilder::new());

    c4u.insert(["gen"], "a1", HeaderBuilder::new());
    c4u.insert(["gen"], "a2", HeaderBuilder::new());

    c4u.insert(["a1", "a2"], "b1", HeaderBuilder::new());
    c4u.insert(["a1", "a2"], "b2", HeaderBuilder::new());

    c4u.insert(["b1", "b2"], "c", HeaderBuilder::new());

    let t0 = c4u.tipset(["gen"]);
    let t1 = c4u.tipset(["a1", "a2"]);
    let t2 = c4u.tipset(["b1", "b2"]);
    let t3 = c4u.tipset(["c"]);

    assert_eq!(t0.epoch(), 0);
    assert_eq!(t1.epoch(), 1);
    assert_eq!(t2.epoch(), 2);
    assert_eq!(t3.epoch(), 3);

    itertools::assert_equal(
        iter::successors(Some(t3.clone()), |t| match t.epoch() {
            0 => None,
            _ => Some(Tipset::load(&c4u, t.parents()).unwrap().unwrap()),
        }),
        [t3, t2, t1, t0],
    );
}

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
    }
}

#[derive(Default)]
pub struct Chain4U<T = MemoryDB> {
    blockstore: T,
    ident2header: ahash::HashMap<String, RawBlockHeader>,
    ident_graph: KeyedDiGraph<String, ()>,
}

impl Chain4U {
    pub fn new() -> Self {
        Default::default()
    }
}

impl<T> Chain4U<T> {
    pub fn with_blockstore(blockstore: T) -> Self {
        Self {
            blockstore,
            ident2header: Default::default(),
            ident_graph: Default::default(),
        }
    }
    pub fn get<Q: ?Sized>(&self, ident: &Q) -> Option<&RawBlockHeader>
    where
        String: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.ident2header.get(ident)
    }
    pub fn tipset<'a>(&self, of: impl IntoIterator<Item = &'a str>) -> Tipset {
        Tipset::new(of.into_iter().map(|it| &self.ident2header[it]).cloned()).unwrap()
    }
}

impl<T: Blockstore> Chain4U<T> {
    pub fn insert<'a>(
        &mut self,
        parents: impl IntoIterator<Item = &'a str>,
        name: impl Into<String>,
        header: impl Into<HeaderBuilder>,
    ) -> &RawBlockHeader {
        let parents = parents.into_iter().collect::<Vec<_>>();
        let name = name.into();
        let mut header: HeaderBuilder = header.into();

        let siblings = parents
            .iter()
            .flat_map(|it| {
                self.ident_graph
                    .neighbors_directed(*it, Direction::Outgoing)
            })
            .unique()
            .map(|it| &self.ident2header[it])
            .collect::<Vec<_>>();

        let parent_tipset =
            match Tipset::new(parents.iter().map(|it| &self.ident2header[*it]).cloned()) {
                Ok(tipset) => Some(tipset),
                Err(CreateTipsetError::Empty) => None,
                Err(e) => panic!("invalid blocks for creating a parent tipset: {e}"),
            };

        // We care about the following properties for creating a valid tipset:
        // - all miners must be unique.
        //   We enforce this by making sure that all children of a block have unique miner addresses.
        // - the epoch must match.
        // - setting the parent tipset

        // There are three sources of values for blocks:
        // - parents
        // - user request
        // - siblings
        //
        // we must make sure that they are compatible, to guard against subtle bugs

        // Epoch
        ////////
        let epoch_from_user = header.epoch.fixed().copied();
        let epoch_from_parents = parent_tipset.as_ref().map(|it| it.epoch() + 1);
        let epoch_from_siblings = match siblings.iter().map(|it| it.epoch).all_equal_value() {
            Ok(epoch) => Some(epoch),
            Err(None) => None,
            Err(Some((left, right))) => panic!("mismatched sibling epochs: {} and {}", left, right),
        };

        let epoch = epoch_from_user
            .or(epoch_from_parents)
            .or(epoch_from_siblings)
            .unwrap_or(0);

        // ensure consistency
        for it in [epoch_from_user, epoch_from_parents, epoch_from_siblings] {
            match it {
                Some(it) if it == epoch => {}
                Some(it) => panic!("inconsistent epoch: {} vs {}", it, epoch),
                None => {}
            }
        }
        header.epoch.insert_or_panic(epoch);

        // Parents and state root
        /////////////////////////
        header.parents.close_with(|| {
            parent_tipset
                .as_ref()
                .map(Tipset::key)
                .cloned()
                .unwrap_or_default()
        });
        header.state_root.close_with(Cid::default);

        // Miner
        ////////
        let sibling_miner_addresses = siblings
            .iter()
            .map(|it| it.miner_address)
            .collect::<Vec<_>>();
        assert!(sibling_miner_addresses.iter().all_unique());
        match header.miner_address {
            Override::Open => {
                header.miner_address = Override::Closed(
                    (0..)
                        .map(Address::new_id)
                        .find(|it| !sibling_miner_addresses.contains(it))
                        .unwrap(),
                )
            }
            Override::Closed(it) | Override::Overridden(it) => {
                assert!(!sibling_miner_addresses.contains(&it))
            }
        }

        // Done! Save it out
        ////////////////////
        let header = header.build(RawBlockHeader::default());

        self.blockstore
            .put_keyed(&header.cid(), &fvm_ipld_encoding::to_vec(&header).unwrap())
            .unwrap();
        for parent in parents {
            self.ident_graph
                .insert_edge(String::from(parent), name.clone(), ())
                .unwrap();
        }
        assert!(!self.ident_graph.contains_cycles());

        match self.ident2header.entry(name) {
            Occupied(it) => panic!("duplicate for key {}", it.key()),
            Vacant(it) => it.insert(header),
        }
    }
}

impl<T: Blockstore> Blockstore for Chain4U<T> {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        self.blockstore.get(k)
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        self.blockstore.put_keyed(k, block)
    }
}

impl<T> Index<&str> for Chain4U<T> {
    type Output = RawBlockHeader;

    fn index(&self, index: &str) -> &Self::Output {
        self.get(index).unwrap()
    }
}

struct KeyedDiGraph<N, E, Ix = petgraph::graph::DefaultIx> {
    node2ix: bimap::BiMap<N, petgraph::graph::NodeIndex<Ix>>,
    graph: petgraph::graph::DiGraph<N, E, Ix>,
}

impl<N, E, Ix> Default for KeyedDiGraph<N, E, Ix>
where
    Ix: petgraph::graph::IndexType,
    N: Hash + Eq,
    Ix: Hash + Eq,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<N, E, Ix> KeyedDiGraph<N, E, Ix> {
    fn new() -> Self
    where
        Ix: petgraph::graph::IndexType,
        N: Hash + Eq,
        Ix: Hash + Eq,
    {
        Self {
            node2ix: bimap::BiMap::new(),
            graph: petgraph::graph::DiGraph::default(),
        }
    }
    fn insert_edge(&mut self, from: N, to: N, weight: E) -> Result<(), &'static str>
    where
        Ix: petgraph::graph::IndexType,
        N: Clone + Eq + Hash,
    {
        let from = self.get_ix(&from);
        let to = self.get_ix(&to);
        match self.graph.contains_edge(from, to) {
            true => Err("edge already in graph"),
            false => {
                self.graph.add_edge(from, to, weight);
                Ok(())
            }
        }
    }
    fn get_ix(&mut self, node: &N) -> petgraph::graph::NodeIndex<Ix>
    where
        Ix: petgraph::graph::IndexType,
        N: Clone + Eq + Hash,
    {
        match self.node2ix.contains_left(node) {
            true => self.node2ix.get_by_left(node).copied().unwrap(),
            false => {
                let ix = self.graph.add_node(node.clone());
                let res = self.node2ix.insert_no_overwrite(node.clone(), ix);
                assert!(res.is_ok());
                ix
            }
        }
    }
    fn contains_cycles(&self) -> bool
    where
        Ix: petgraph::graph::IndexType,
    {
        petgraph::algo::is_cyclic_directed(&self.graph)
    }
    fn neighbors_directed<Q: ?Sized>(
        &self,
        node: &Q,
        dir: petgraph::Direction,
    ) -> impl Iterator<Item = &N>
    where
        Ix: petgraph::graph::IndexType,
        N: Borrow<Q> + Hash + Eq,
        Q: Hash + Eq,
    {
        self.node2ix
            .get_by_left(node)
            .into_iter()
            .flat_map(move |ix| self.graph.neighbors_directed(*ix, dir))
            .map(|ix| self.node2ix.get_by_right(&ix).unwrap())
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
    ($($setter_name:ident/$clearer_name:ident -> $field_name:ident: $field_ty:ty);* $(;)?) => {
        $(
            pub fn $clearer_name(&mut self) -> &mut Self {
                self.$field_name = Override::Open;
                self
            }
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
        with_miner_address / without_miner_address -> miner_address: Address;
        with_ticket / without_ticket -> ticket: Option<Ticket>;
        with_election_proof / without_election_proof -> election_proof: Option<ElectionProof>;
        with_beacon_entries / without_beacon_entries -> beacon_entries: Vec<BeaconEntry>;
        with_winning_post_proof / without_winning_post_proof -> winning_post_proof: Vec<PoStProof>;
        with_parents / without_parents -> parents: TipsetKey;
        with_weight / without_weight -> weight: BigInt;
        with_epoch / without_epoch -> epoch: ChainEpoch;
        with_state_root / without_state_root -> state_root: Cid;
        with_message_receipts / without_message_receipts -> message_receipts: Cid;
        with_messages / without_messages -> messages: Cid;
        with_bls_aggregate / without_bls_aggregate -> bls_aggregate: Option<Signature>;
        with_timestamp / without_timestamp -> timestamp: u64;
        with_signature / without_signature -> signature: Option<Signature>;
        with_fork_signal / without_fork_signal -> fork_signal: u64;
        with_parent_base_fee / without_parent_base_fee -> parent_base_fee: TokenAmount;
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

impl From<&RawBlockHeader> for HeaderBuilder {
    fn from(value: &RawBlockHeader) -> Self {
        Self::from(RawBlockHeader::clone(value))
    }
}
