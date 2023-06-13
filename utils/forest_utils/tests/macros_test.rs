// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;

use forest_utils::retry;
use log::info;
use tokio::time::sleep;

#[allow(clippy::unused_async)]
async fn failing_function() -> Result<(), ()> {
    Err(())
}

#[allow(clippy::unused_async)]
async fn successful_function() -> Result<(), ()> {
    Ok(())
}

#[allow(clippy::unused_async)]
async fn retryable_function(counter: &mut i32) -> Result<(), ()> {
    if *counter > 0 {
        *counter -= 1;
        Err(())
    } else {
        Ok(())
    }
}

#[tokio::test]
// Tests that the retry macro correctly handles a function that always fails.
// This case should return Err after the maximum number of retries is exceeded.
async fn test_retry_macro_failing_function() {
    let result = retry!(failing_function, 3, Duration::from_nanos(0));
    assert!(result.is_err());
}

#[tokio::test]
// Tests that the retry macro correctly handles a function that always succeeds.
// This case should return Ok without retrying the function.
async fn test_retry_macro_successful_function() {
    let result = retry!(successful_function, 3, Duration::from_nanos(0));
    assert!(result.is_ok());
}

#[tokio::test]
// Tests that the retry macro correctly handles a function that may fail a few
// times before succeeding. This case should return Ok after retrying the
// function a few times and modifying the argument that is passed to the
// function.
async fn test_retry_macro_retryable_function() {
    let mut counter = 3;
    let result = retry!(retryable_function, 5, Duration::from_nanos(0), &mut counter);
    assert!(result.is_ok());
    assert_eq!(counter, 0);
}
