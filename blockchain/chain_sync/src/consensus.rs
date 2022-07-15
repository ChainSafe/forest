// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use async_std::stream::StreamExt;
use async_std::task;
use async_trait::async_trait;
use futures::stream::FuturesUnordered;
use nonempty::NonEmpty;
use state_manager::StateManager;
use std::{
    fmt::{Debug, Display},
    sync::Arc,
};

use forest_blocks::Block;
use ipld_blockstore::BlockStore;

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
pub trait Consensus: Debug + Send + Sync + Unpin + 'static {
    type Error: Debug + Display + Send + Sync;

    /// Perform block validation asynchronously and return all encountered errors if failed.
    ///
    /// Being asynchronous gives the method a chance to construct a pipeline of
    /// validations, ie. do some common ones before branching out.
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
