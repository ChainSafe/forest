// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use async_std::task;
use async_std::{channel::Sender, stream::StreamExt};
use async_trait::async_trait;
use forest_chain::Scale;
use forest_libp2p::{NetworkMessage, Topic, PUBSUB_BLOCK_STR};
use forest_message::SignedMessage;
use forest_message_pool::MessagePool;
use futures::stream::FuturesUnordered;
use fvm_ipld_encoding::Cbor;
use nonempty::NonEmpty;
use std::borrow::Cow;
use std::{
    fmt::{Debug, Display},
    sync::Arc,
};

use forest_blocks::{Block, GossipBlock, Tipset};
use forest_ipld_blockstore::BlockStore;
use forest_state_manager::StateManager;

/// The `Consensus` trait encapsulates consensus specific rules of validation
/// and block creation. Behind the scenes they can farm out the total ordering
/// of transactions to an arbitrary consensus engine, but in the end they
/// package the transactions into Filecoin compatible blocks.
///
/// Not all fields will be made use of, however, so the validation of these
/// blocks at least partially have to be trusted to the `Consensus` component.
///
/// Common rules for message ordering will be followed, and can be validated
/// outside by the host system during chain synchronization.
#[async_trait]
pub trait Consensus: Scale + Debug + Send + Sync + Unpin + 'static {
    type Error: Debug + Display + Send + Sync;

    /// Perform block validation asynchronously and return all encountered errors if failed.
    ///
    /// Being asynchronous gives the method a chance to construct a pipeline of
    /// validations, i.e. do some common ones before branching out.
    async fn validate_block<DB>(
        &self,
        state_manager: Arc<StateManager<DB>>,
        block: Arc<Block>,
    ) -> Result<(), NonEmpty<Self::Error>>
    where
        DB: BlockStore + Sync + Send + 'static;
}

/// Helper function to collect errors from async validations.
pub async fn collect_errs<E>(
    mut handles: FuturesUnordered<task::JoinHandle<Result<(), E>>>,
) -> Result<(), NonEmpty<E>> {
    let mut errors = Vec::new();

    while let Some(result) = handles.next().await {
        if let Err(e) = result {
            errors.push(e);
        }
    }

    let mut errors = errors.into_iter();

    match errors.next() {
        None => Ok(()),
        Some(head) => Err(NonEmpty {
            head,
            tail: errors.collect(),
        }),
    }
}

/// The `Proposer` trait expresses the ability to "mine", or in more general,
/// to propose blocks to the network according to the rules of the consensus
/// protocol.
///
/// It is separate from the `Consensus` trait because it is only expected to
/// be called once, then left to run in the background and try to publish
/// blocks to the network which may or may not be adopted.
///
/// It exists mostly as a way for us to describe what kind of dependencies
/// mining processes are expected to take.
#[async_trait]
pub trait Proposer {
    /// Start proposing blocks in the background and never return, unless
    /// something went horribly wrong. Broadly, they should select messages
    /// from the `mempool`, come up with a total ordering for them, then create
    /// blocks and publish them to the network.
    ///
    /// To establish total ordering, the proposer might have to communicate
    /// with other peers using custom P2P messages, however that is its own
    /// concern, the dependencies to implement a suitable network later must
    /// come from somewhere else, because they are not common across all
    /// consensus variants.
    async fn run<DB, MP>(
        self,
        // NOTE: We will need access to the `ChainStore` as well, or, ideally
        // a wrapper over it that only allows us to do what we need to, but
        // for example not reset the Genesis. But since the `StateManager`
        // already exposes the `ChainStore` as is, and this is how it's
        // accessed during validation for example, I think we can defer
        // these for later refactoring and just use the same pattern.
        state_manager: Arc<StateManager<DB>>,
        mpool: &MP,
        submitter: &SyncGossipSubmitter,
    ) -> anyhow::Result<()>
    where
        DB: BlockStore + Sync + Send + 'static,
        MP: MessagePoolApi + Sync + Send + 'static;
}

/// The `MessagePoolApi` is the window of consensus to the contents of the `MessagePool`.
///
/// It exists to narrow down the possible operations that a consensus engine can do with
/// the `MessagePool` to only those that it should reasonably exercise, which are mostly
/// read-only queries to get transactions which can be expected to be put in the next
/// block, based on their account nonces and the current state.
///
/// The `MessagePool` is still expected to monitor the chain growth and remove messages
/// which were included in blocks on its own.
#[async_trait]
pub trait MessagePoolApi {
    /// Select the set of suitable signed messages based on a tipset we are about
    /// to build the next block on.
    ///
    /// The result is a `Cow` in case the source can avoid cloning messages and just
    /// return a reference. They will be sent to the data store for storage, but a
    /// reference is enough for that.
    async fn select_signed<DB>(
        &self,
        state_manager: &StateManager<DB>,
        base: &Tipset,
    ) -> anyhow::Result<Vec<Cow<SignedMessage>>>
    where
        DB: BlockStore + Sync + Send + 'static;
}

#[async_trait]
impl<P> MessagePoolApi for MessagePool<P>
where
    P: forest_message_pool::Provider + Send + Sync + 'static,
{
    async fn select_signed<DB>(
        &self,
        _: &StateManager<DB>,
        base: &Tipset,
    ) -> anyhow::Result<Vec<Cow<SignedMessage>>>
    where
        DB: BlockStore + Sync + Send + 'static,
    {
        self.select_messages_for_block(base)
            .await
            .map_err(|e| e.into())
            .map(|v| v.into_iter().map(Cow::Owned).collect())
    }
}

/// `SyncGossipSubmitter` dispatches proposed blocks to the network and the local chain synchronizer.
///
/// Similar to `sync_api::sync_submit_block` but assumes that the block is correct and already persisted.
pub struct SyncGossipSubmitter {
    network_name: String,
    network_tx: Sender<NetworkMessage>,
    tipset_tx: Sender<Arc<Tipset>>,
}

impl SyncGossipSubmitter {
    pub fn new(
        network_name: String,
        network_tx: Sender<NetworkMessage>,
        tipset_tx: Sender<Arc<Tipset>>,
    ) -> Self {
        Self {
            network_name,
            network_tx,
            tipset_tx,
        }
    }

    pub async fn submit_block(&self, block: GossipBlock) -> anyhow::Result<()> {
        let data = block.marshal_cbor()?;
        let ts = Arc::new(Tipset::new(vec![block.header])?);
        let msg = NetworkMessage::PubsubMessage {
            topic: Topic::new(format!("{}/{}", PUBSUB_BLOCK_STR, self.network_name)),
            message: data,
        };
        self.tipset_tx.send(ts).await?;
        self.network_tx.send(msg).await?;
        Ok(())
    }
}
