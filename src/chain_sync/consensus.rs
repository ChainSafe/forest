// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use nunny::Vec as NonEmpty;
use tokio::task::JoinSet;

/// Helper function to collect errors from async validations.
pub async fn collect_errs<E: 'static>(
    mut handles: JoinSet<Result<(), E>>,
) -> Result<(), NonEmpty<E>> {
    let mut errors = Vec::new();

    while let Some(result) = handles.join_next().await {
        if let Ok(Err(e)) = result {
            errors.push(e);
        }
    }

    match errors.try_into() {
        Ok(it) => Err(it),
        Err(_) => Ok(()),
    }
}
