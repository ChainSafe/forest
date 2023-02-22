// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;

use forest_cli_shared::retry;
use log::info;
use tokio::time::sleep;

async fn failing_function() -> Result<(), ()> {
    Err(())
}

async fn successful_function() -> Result<(), ()> {
    Ok(())
}

async fn retryable_function(counter: &mut i32) -> Result<(), ()> {
    if *counter > 0 {
        *counter -= 1;
        Err(())
    } else {
        Ok(())
    }
}

#[tokio::test]
async fn test_retry_macro_failing_function() {
    let result = retry!(failing_function, 3, Duration::from_millis(100));
    assert!(result.is_err());
}

#[tokio::test]
async fn test_retry_macro_successful_function() {
    let result = retry!(successful_function, 3, Duration::from_millis(100));
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_retry_macro_retryable_function() {
    let mut counter = 3;
    let result = retry!(
        retryable_function,
        5,
        Duration::from_millis(100),
        &mut counter
    );
    assert!(result.is_ok());
    assert_eq!(counter, 0);
}
