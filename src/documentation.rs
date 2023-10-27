// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This is an empty module for documentation purposes.
//!
//! Documentation of core concepts belong in-tree (in `/src`).
//!
//! Unless it better fits on a specific component, this module is a the place for
//! documentation about:
//! - The current behavior of Forest
//! - Filecoin concepts
//!
//! Documentation that doesn't fit the above should be in `/documentation`:
//! - User-facing guides.
//! - Developer _process_ guides, i.e memory profiling, release checklists.

/// This is a ground-up introduction to the different kinds of snapshot files,
/// covering:
/// 1. [Actors in Filecoin](#actors).
/// 2. [The Filecoin Blockchain](#the-filecoin-blockchain)
/// 3. [The Filecoin State Tree](#the-filecoin-state-tree)
/// 4. (Finally) [snapshots](#snapshots)
///
/// # Actors
///
/// The Filecoin Virtual Machine (FVM) hosts a number of _actors_.
/// These are objects that maintain and mutate internal state, and communicate
/// by passing messages.
///
/// An example of an actor is the [`cron`](fil_actors_shared::v11::runtime::builtins::Type::Cron)
/// actor.
/// Its [internal state](fil_actor_cron_state::v11::State) is a to-do list of
/// other actors to invoke every epoch.
///
/// See [the Filecoin docs](https://docs.filecoin.io/basics/the-blockchain/actors)
/// for more information about actors.
///
/// # The Filecoin blockchain
///
/// Filecoin consists of a blockchain of `messages`.
/// Listed below are the core objects for the blockchain.
/// Each one can be addressed by a [`Cid`](cid::Cid).
///
/// - [`Message`](crate::shim::message::Message)s are statements of messages between
///   the actors.
///   They describe and (equivalently) represent a change in _the state tree_ (see below).
///   See [`apply_block_messages`](crate::state_manager::apply_block_messages) to learn
///   more.
///   Messages may be [signed](crate::message::SignedMessage).
/// - `Message`s are grouped into [`Block`](crate::blocks::Block)s, with a single
///   [`BlockHeader`](crate::blocks::BlockHeader).
///   These are what are mined by miners to get `FIL` (money).
///   They define an [_epoch_](crate::blocks::BlockHeader::epoch) and a
///   [_parent tipset_](crate::blocks::BlockHeader::parents).
///   The _epoch_ is a monotonically increasing number from `0` (genesis).
/// - `Block`s are grouped into [`Tipset`](crate::blocks::Tipset)s.
///   All blocks in a tipset share the same `epoch`.
///
/// ```text
///      ┌───────────────────────────────┐
///      │ BlockHeader { epoch:  0, .. } │ //  The genesis block/tipset
///   ┌● └───────────────────────────────┘
///   ~
///   └──┬───────────────────────────────┐
///      │ BlockHeader { epoch: 10, .. } │ // The epoch 10 tipset - one block with two messages
///   ┌● └┬──────────────────────────────┘
///   │   │
///   │   │ "I contain the following messages..."
///   │   │
///   │   ├──────────────────┐
///   │   │ ┌──────────────┐ │ ┌───────────────────┐
///   │   └►│ Message:     │ └►│ Message:          │
///   │     │  Afri -> Bob │   │  Charlie -> David │
///   │     └──────────────┘   └───────────────────┘
///   │
///   │ "my parent is..."
///   │
///   └──┬───────────────────────────────┐
///      │ BlockHeader { epoch: 11, .. } │ // The epoch 11 tipset - one block with one message
///   ┌● └┬──────────────────────────────┘
///   │   │ ┌────────────────┐
///   │   └►│ Message:       │
///   │     │  Eric -> Frank │
///   │     └────────────────┘
///   │
///   │ // the epoch 12 tipset - two blocks, with a total of 3 messages
///   │
///   ├────────────────────────────────────┐
///   └──┬───────────────────────────────┐ └─┬───────────────────────────────┐
///      │ BlockHeader { epoch: 12, .. } │   │ BlockHeader { epoch: 12, .. } │
///   ┌● └┬──────────────────────────────┘   └┬─────────────────────┬────────┘
///   ~   │ ┌───────────────────────┐         │ ┌─────────────────┐ │ ┌──────────────┐
///       └►│ Message:              │         └►│ Message:        │ └►│ Message:     │
///         │  Guillaume -> Hailong │           │  Hubert -> Ivan │   │  Josh -> Kai │
///         └───────────────────────┘           └─────────────────┘   └──────────────┘
/// ```
///
/// The [`ChainMuxer`](crate::chain_sync::ChainMuxer) receives two kinds of [messages](crate::libp2p::PubsubMessage)
/// from peers:
/// - [`GossipBlock`](crate::blocks::GossipBlock)s are descriptions of a single block, with the `BlockHeader` and `Message` CIDs.
/// - [`SignedMessage`](crate::message::SignedMessage)s
///
/// It assembles these messages into a chain to genesis.
///
/// Filecoin implementations store all the above in the `ChainStore`, per
/// [the spec](https://github.com/filecoin-project/specs/blob/936f07f9a444036fe86442c919940ea0e4fb0a0b/content/systems/filecoin_nodes/repository/ipldstore/_index.md?plain=1#L43-L50).
///
/// # The Filecoin state tree
///
/// `Message`s describe/represent mutations in the [`StateTree`](crate::shim::state_tree::StateTree),
/// which is a representation of all Filecoin state at a point in time.
/// For each actor, the `StateTree` holds the CID for its state: [`ActorState.state`](fvm4::state_tree::ActorState::state).
///
/// Actor state is serialized and stored as  [`Ipld`](libipld::Ipld).
/// Think of this as "JSON with links ([`Cid`](cid::Cid)s)".
/// So the `cron` actor's state mentioned above will be ultimately serialized into `Ipld`
/// and stored in the `StateStore`, per
/// [the spec](https://github.com/filecoin-project/specs/blob/936f07f9a444036fe86442c919940ea0e4fb0a0b/content/systems/filecoin_nodes/repository/ipldstore/_index.md?plain=1#L43-L50).
///
/// It isn't feasible to create a new copy of actor states whenever they change.
/// That is, in a fictional [^1] example of a `cron` actor, starting with a [`crontab`](https://man7.org/linux/man-pages/man5/crontab.5.html)
/// with 10 items, mutation of the state should _not_ simply duplicate the state:
/// ```text
/// Previous state             Current state
/// ┌───────────────────────┐  ┌───────────────────────┐
/// │Crontab                │  │Crontab                │
/// │1. Get out of bed      │  │1. Get out of bed      │
/// │2. Shower              │  │2. Shower              │
/// │...                    │  │...                    │
/// │10. Take over the world│  │10. Take over the world│
/// └───────────────────────┘  │11. Throw a party      │
///                            └───────────────────────┘
/// ```
/// But should instead be able to refer to the previous state:
/// ```text
/// Previous state             Current state
/// ┌───────────────────────┐  ┌─────────────────┐
/// │Crontab                │◄─┤(See CID...)     │
/// │1. Get out of bed      │  ├─────────────────┤
/// │2. Shower              │  │11. Throw a party│
/// │...                    │  └─────────────────┘
/// │10. Take over the world│
/// └───────────────────────┘
/// ```
/// And removal of e.g the latest entry works similarly, _orphaning_ the removed
/// item.
/// ```text
/// Previous state             Orphaned item        Current state
/// ┌───────────────────────┐                       ┌────────────┐
/// │Crontab                │◄──────────────────────┤(See CID...)│
/// │1. Get out of bed      │  ┌─────────────────┐  └────────────┘
/// │2. Shower              │  │11. Throw a party│
/// │...                    │  └─────────────────┘
/// │10. Take over the world│
/// └───────────────────────┘
/// ```
///
/// [^1]: The real `cron` actor doesn't mutate state like this.
///
/// Data structures that reach into the past of the `StateStore` like this are:
/// - ["AMT"](fil_actors_shared::fvm_ipld_amt), a list.
/// - ["HAMT"](fil_actors_shared::fvm_ipld_hamt), a map.
///
/// Therefore, the Filecoin state is, indeed, a tree of IPLD data.
/// It can be addressed by the root of the tree, so it is often referred to as
/// the _state root_.
///
/// We will now introduce some new terminology given the above information.
///
/// With respect to a particular IPLD [`Blockstore`](fvm_ipld_blockstore::Blockstore):
/// - An item such a list is _fully inhabited_ if all its recursive
///   [`Ipld::Link`](libipld::Ipld::Link)s exist in the blockstore.
/// - Otherwise, an item is only _partially inhabited_.
///   The links are said to be "dead links".
///
/// With respect to a particular `StateTree`:
/// - An item is _orphaned_ if it is not reachable from the current state tree
///   through any links.
///
/// # Snapshots
///
/// Recall that for each message execution, the state tree is mutated.
/// Therefore, each epoch is associated with a state tree after execution,
/// and a [_parent state tree_](crate::blocks::BlockHeader::state_root).
///
/// ```text
///                                            // state after execution of
///                                            // all messages in that epoch
///      ┌───────────────────────────────┐ ┌────────────┐
///      │ BlockHeader { epoch:  0, .. } │ │ state root ├──► initial actor states...
///   ┌● └───────────────────────────────┘ └────────────┘                    ▲   ▲
///   ~                                        // links to redundant data ─● │   │
///   └──┬───────────────────────────────┐ ┌────────────┐                    │   │
///      │ BlockHeader { epoch: 11, .. } │ │ state root ├─┬► actor state ─► AMT  │
///   ┌● └┬──────────────────────────────┘ └────────────┘ ~                      │
///   │   │ ┌─────────┐                                   └► actor state ─► HAMT ┘
///   │   └►│ Message │                                                      │
///   │     └─────────┘                                                      ▼
///   ├──┬───────────────────────────────┐     // new data in this epoch ─● IPLD
///   │  │ BlockHeader { epoch: 12, .. } │
///   │  └┬─────────────┬────────────────┘
///   │   │ ┌─────────┐ │ ┌─────────┐
///   │   └►│ Message │ └►│ Message │
///   │     └─────────┘   └─────────┘                                        ~   ~
///   └──┬───────────────────────────────┐ ┌────────────┐                    │   │
///      │ BlockHeader { epoch: 12, .. } │ │ state root ├─┬► actor state ─► AMT  │
///   ┌● └┬──────────────────────────────┘ └────────────┘ ~                      │
///   ~   │ ┌─────────┐                                   └► actor state ─► HAMT ┘
///       └►│ Message │
///         └─────────┘
/// ```
///
/// We are now ready to define the different snapshot types for a given epoch N.
/// - A _lite snapshot_ contains:
///   - All block headers from genesis to epoch N.
///   - For the last W (width) epochs:
///     - The _fully inhabited_ state trees.
///     - The messages.
///   - For epochs 0..N-W, the state trees will be dead or partially inhabited.
/// - A _full snapshot_ contains:
///   - All block headers from genesis to epoch N.
///   - The fully inhabited state trees for epoch 0..N
/// - A _diff snapshot_ contains:
///   - For epoch N-W..N:
///     - The block headers.
///     - The messages.
///     - New data in that epoch, which will be partially inhabited
///
/// Successive diff snapshots may be concatenated:
/// - From genesis, to produce a full snapshot.
/// - From a lite snapshot, to produce a successive lite snapshot.
mod snapshots {}
