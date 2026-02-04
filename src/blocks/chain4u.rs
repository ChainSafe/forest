// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains [`Chain4U`] and [`chain4u!`], which together provide a
//! declarative way of creating test chains.
//!
//! See the [`api_walkthrough`] test.

use crate::{
    beacon::BeaconEntry,
    blocks::*,
    db::{MemoryDB, car::PlainCar},
    networks,
    shim::{
        address::Address, clock::ChainEpoch, crypto::Signature, econ::TokenAmount,
        sector::PoStProof,
    },
};
use chain4u::header::{FILECOIN_GENESIS_BLOCK, FILECOIN_GENESIS_CID, GENESIS_BLOCK_PARENTS};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use itertools::Itertools as _;
use num_bigint::BigInt;
use petgraph::Direction;
use std::{borrow::Borrow, fmt::Debug, hash::Hash};
use std::{
    collections::hash_map::Entry::{Occupied, Vacant},
    iter,
};

/// Declarative API for creating test tipsets.
///
/// See [module documentation](mod@self) for more.
macro_rules! chain4u {
    (
        $(from [$($fork_header:ident),* $(,)?])?
        in $c4u:expr;
        $(
            $($tipset:ident @)? [
                $(
                    $header:ident
                    $(= $init:expr)?
                ),+ $(,)?
            ]
        )->*
    ) => {
        let __c4u: &$crate::blocks::Chain4U<_> = &$c4u;
        let mut __running_parent: &[&str] = &[];
        $(let mut __running_parent: &[&str] = &[$(stringify!($fork_header),)*];)?

        $(
            $(
                let $header: &$crate::blocks::RawBlockHeader =
                    &__c4u.insert(
                        __running_parent,
                        stringify!($header),
                        {
                            let _init = $crate::blocks::HeaderBuilder::new();
                            $(let _init = $crate::blocks::HeaderBuilder::from($init);)?
                            _init
                        }
                    );
            )*

            __running_parent = &[
                $(stringify!($header),)*
            ];

            $(let $tipset: &$crate::blocks::Tipset = &__c4u.tipset(__running_parent);)?
        )*
    };
}
pub(crate) use chain4u;

#[test]
#[allow(unused_variables)]
fn api_walkthrough() {
    // create a c4u context.
    // this stores chain information in between macro invocations,
    // (and also actually creates the block headers and tipsets).
    let c4u = Chain4U::new();

    chain4u! {
        in c4u; // select the context
        [genesis_header]  // square brackets `[..]` surround each tipset
        -> [first_header] // a `&RawBlockHeader` is bound to each name in a tipset
        -> [second_left, second_right] // multiple blocks in a tipset
        -> [third]
        -> t4 @ [fourth]  // a `&Tipset` is bound to the optional `name @` sigil
    };

    assert_eq!(genesis_header.epoch, 0); // a root header was generated
    assert_eq!(first_header.epoch, 1); // and chained blocks appropriately
    assert_ne!(second_left, second_right); // siblings are distinct
    assert_eq!(t4.epoch(), 4);

    // you can continue building chains in later invocations
    chain4u! {
        from [fourth] in c4u;
        [fifth = HeaderBuilder::new().with_timestamp(100)] // you can set certain fields
        -> t6 @ [
            sixth_left,
            sixth_right = HeaderBuilder {
                miner_address: Address::new_id(100).into(),
                ..Default::default()
            }
        ]
    };

    assert_eq!(fifth.epoch, 5);
    assert_eq!(fifth.timestamp, 100);
    assert_eq!(sixth_right.miner_address, Address::new_id(100));
    assert_eq!(t6.block_headers().len(), 2);

    // this can be used to create forks
    chain4u! {
        from [third] in c4u;
        [fourth_fork]
        -> [fifth_fork]
    };

    assert_eq!(fourth_fork.epoch, 4);
    assert_ne!(fourth, fourth_fork); // fork siblings are distinct

    chain4u! {
        in c4u;
        [calib_gen = calibnet_genesis()]
        // if you provide a full blockheader, it will be preserved exactly
        // (and will panic the harness if it cannot be preserved while e.g
        // incrementing the epoch number).
        -> [calib_first]
    };

    assert_eq!(calib_gen.clone(), calibnet_genesis());
}

fn calibnet_genesis() -> RawBlockHeader {
    PlainCar::new(networks::calibnet::DEFAULT_GENESIS)
        .unwrap()
        .get_cbor(&networks::calibnet::GENESIS_CID)
        .unwrap()
        .unwrap()
}

/// Context for creating test tipsets.
///
/// See [module documentation](mod@self) for more.
#[derive(Default)]
pub struct Chain4U<T = MemoryDB> {
    blockstore: T,
    /// [`Blockstore`]s are typically behind a shared reference, e.g as an `Arc<DB>`
    /// inside `ChainStore`, so we have to have interior mutability too.
    inner: parking_lot::Mutex<Chain4UInner>,
}

impl Chain4U {
    pub fn new() -> Self {
        Default::default()
    }
}

impl<T> Chain4U<T> {
    pub fn with_blockstore(blockstore: T) -> Self
    where
        T: Blockstore,
    {
        blockstore
            .put_keyed(&FILECOIN_GENESIS_CID, &FILECOIN_GENESIS_BLOCK)
            .unwrap();
        Self {
            blockstore,
            inner: Default::default(),
        }
    }
    pub fn get<Q>(&self, ident: &Q) -> Option<RawBlockHeader>
    where
        String: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.inner.lock().ident2header.get(ident).cloned()
    }
    pub fn tipset(&self, of: &[&str]) -> Tipset {
        Tipset::new(of.iter().map(|it| self.get(*it).unwrap())).unwrap()
    }
    /// Insert a header.
    /// Header fields (epoch etc) will be set accordingly.
    pub fn insert(
        &self,
        parents: &[&str],
        name: impl Into<String>,
        header: HeaderBuilder,
    ) -> RawBlockHeader
    where
        T: Blockstore,
    {
        let header = self
            .inner
            .lock()
            .insert(parents, name.into(), header)
            .clone();
        self.blockstore
            .put_keyed(&header.cid(), &fvm_ipld_encoding::to_vec(&header).unwrap())
            .unwrap();
        header
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

#[derive(Default)]
struct Chain4UInner {
    ident2header: ahash::HashMap<String, RawBlockHeader>,
    ident_graph: KeyedDiGraph<String, ()>,
}

impl Chain4UInner {
    fn insert(
        &mut self,
        parents: &[&str],
        name: String,
        mut header: HeaderBuilder,
    ) -> &RawBlockHeader {
        let siblings = parents
            .iter()
            .flat_map(|it| {
                self.ident_graph
                    .neighbors_directed(*it, Direction::Outgoing)
            })
            .unique()
            .map(|it| &self.ident2header[it])
            .collect_vec();

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
        let epoch_from_user = header.epoch.into_fixed();
        let epoch_from_parents = parent_tipset.as_ref().map(|it| it.epoch() + 1);
        let epoch_from_siblings = match siblings.iter().map(|it| it.epoch).all_equal_value() {
            Ok(epoch) => Some(epoch),
            Err(None) => None,
            Err(Some((left, right))) => panic!("mismatched sibling epochs: {left} and {right}"),
        };

        let epoch = epoch_from_user
            .or(epoch_from_parents)
            .or(epoch_from_siblings)
            .unwrap_or(0);

        // ensure consistency
        for it in [epoch_from_user, epoch_from_parents, epoch_from_siblings] {
            match it {
                Some(it) if it <= epoch => {}
                Some(it) => panic!("inconsistent epoch: {it} vs {epoch}"),
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
                .unwrap_or_else(|| GENESIS_BLOCK_PARENTS.clone())
        });
        header.state_root.close_with(Cid::default);

        // Message root
        ///////////////
        header.messages.close_with(|| {
            use crate::utils::db::CborStoreExt as _;
            use fil_actors_shared::fvm_ipld_amt::Amtv0 as Amt;

            let blockstore = MemoryDB::default();
            let bls_message_root = Amt::<Cid, _>::new(&blockstore).flush().unwrap();
            let secp_message_root = Amt::<Cid, _>::new(&blockstore).flush().unwrap();
            let meta = TxMeta {
                bls_message_root,
                secp_message_root,
            };

            // Store message roots and receive meta_root CID
            blockstore.put_cbor_default(&meta).unwrap()
        });

        // Miner
        ////////
        let sibling_miner_addresses = siblings.iter().map(|it| it.miner_address).collect_vec();
        assert!(sibling_miner_addresses.iter().all_unique());
        match header.miner_address.inner {
            None => {
                header.miner_address = Override::new(
                    (0..)
                        .map(Address::new_id)
                        .find(|it| !sibling_miner_addresses.contains(it))
                        .unwrap(),
                )
            }
            Some(already) => {
                assert!(!sibling_miner_addresses.contains(&already))
            }
        }

        // Done! Save it out
        ////////////////////
        let header = header.build(RawBlockHeader::default());

        for parent in parents {
            self.ident_graph
                .insert_edge(String::from(*parent), name.clone(), ())
                .unwrap();
        }
        assert!(!self.ident_graph.contains_cycles());

        match self.ident2header.entry(name) {
            Occupied(it) => panic!("duplicate for key {}", it.key()),
            Vacant(it) => it.insert(header),
        }
    }
}

/// Juggling node and edge indices is tedious - this abstracts that away.
///
/// A [`petgraph::graphmap::GraphMap`] with a non-[`Copy`] index.
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
    fn neighbors_directed<Q>(&self, node: &Q, dir: petgraph::Direction) -> impl Iterator<Item = &N>
    where
        Ix: petgraph::graph::IndexType,
        N: Borrow<Q> + Hash + Eq,
        Q: Hash + Eq + ?Sized,
    {
        self.node2ix
            .get_by_left(node)
            .into_iter()
            .flat_map(move |ix| self.graph.neighbors_directed(*ix, dir))
            .map(|ix| self.node2ix.get_by_right(&ix).unwrap())
    }
}

/// A value which [`Chain4U`] is allowed to change to create tipsets, or is fixed
/// and can't be altered by [`Chain4U`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Override<T> {
    inner: Option<T>,
}

impl<T> Default for Override<T> {
    fn default() -> Self {
        Self { inner: None }
    }
}

impl<T> From<T> for Override<T> {
    fn from(value: T) -> Self {
        Self { inner: Some(value) }
    }
}

impl<T> Override<T> {
    pub fn new(it: T) -> Self {
        Self::from(it)
    }
    fn insert_or_panic(&mut self, it: T)
    where
        T: PartialEq,
    {
        match &self.inner {
            None => self.inner = Some(it),
            Some(already) => match already == &it {
                true => {}
                false => panic!("incompatible value already in `Override`"),
            },
        }
    }
    fn into_fixed(self) -> Option<T> {
        self.inner
    }
    fn close_with(&mut self, with: impl FnOnce() -> T) {
        self.inner.get_or_insert_with(with);
    }
}

/// [`Chain4U`] will change [`RawBlockHeader`] fields to create a valid graph of tipsets.
///
/// This struct describes which fields are _allowed_ to change.
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
    /// Create a new [`HeaderBuilder`], where [`Chain4U`] may change the value of
    /// any of the fields
    pub fn new() -> Self {
        Self::default()
    }
}

macro_rules! setters {
    ($($setter_name:ident/$clearer_name:ident -> $field_name:ident: $field_ty:ty);* $(;)?) => {
        $(
            /// Fix the value of this field so that [`Chain4U`] cannot change it.
            pub fn $setter_name(&mut self, it: $field_ty) -> &mut Self {
                self.$field_name = Override::new(it);
                self
            }
            /// Allow [`Chain4U`] to change the value of this field.
            pub fn $clearer_name(&mut self) -> &mut Self {
                self.$field_name = Override::default();
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
    /// Creates a [`HeaderBuilder`] where [`Chain4U`] may not alter any of the fields
    /// (i.e they will be preserved exactly in the generated chain).
    fn from(value: RawBlockHeader) -> Self {
        macro_rules! overriden {
            ($($ident:ident),* $(,)?) => {
                let RawBlockHeader {
                    $($ident,)*
                } = value;
                Self {
                    $($ident: Override::new($ident),)*
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

// Mixing up `Clone` and `From` is a bit discouraged, but worth it for test ergonomics
impl From<&RawBlockHeader> for HeaderBuilder {
    fn from(value: &RawBlockHeader) -> Self {
        Self::from(RawBlockHeader::clone(value))
    }
}

impl From<CachingBlockHeader> for HeaderBuilder {
    fn from(value: CachingBlockHeader) -> Self {
        Self::from(value.into_raw())
    }
}

impl From<&CachingBlockHeader> for HeaderBuilder {
    fn from(value: &CachingBlockHeader) -> Self {
        Self::from(value.clone().into_raw())
    }
}

impl From<&Self> for HeaderBuilder {
    fn from(value: &Self) -> Self {
        value.clone()
    }
}
impl From<&mut Self /* from the builder methods */> for HeaderBuilder {
    fn from(value: &mut Self) -> Self {
        value.clone()
    }
}

#[test]
#[allow(unused_variables)]
fn test_chain4u_macro() {
    let c4u = Chain4U::new();
    chain4u! {
        in c4u;
        t0 @ [genesis]
        -> ta @ [a1, a2 = HeaderBuilder::new()]
        ->      [b1, b2]
        -> tc @ [c]
    };
    chain4u! {
        from [a1, a2] in c4u;
        [x1, x2]
        -> ty @ [y]
    }

    assert_eq!(t0.epoch(), 0);
    assert_eq!(ta.epoch(), 1);
    assert_eq!(tc.epoch(), 3);

    assert_eq!(ty.epoch(), 3);

    assert_ne!(tc, ty);
    assert!([b1, b2, x1, x2].iter().all_unique());
}

#[test]
fn test_chain4u() {
    let c4u = Chain4U::new();
    c4u.insert(&[], "gen", HeaderBuilder::new());

    c4u.insert(&["gen"], "a1", HeaderBuilder::new());
    c4u.insert(&["gen"], "a2", HeaderBuilder::new());

    c4u.insert(&["a1", "a2"], "b1", HeaderBuilder::new());
    c4u.insert(&["a1", "a2"], "b2", HeaderBuilder::new());

    c4u.insert(&["b1", "b2"], "c", HeaderBuilder::new());

    let t0 = c4u.tipset(&["gen"]);
    let t1 = c4u.tipset(&["a1", "a2"]);
    let t2 = c4u.tipset(&["b1", "b2"]);
    let t3 = c4u.tipset(&["c"]);

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
