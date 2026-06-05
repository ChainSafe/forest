// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use futures::StreamExt as _;

#[tokio::test]
async fn test_has_subscribers() {
    let (tx, mut rx1) = tokio::sync::broadcast::channel::<u32>(16);
    let mut rx2 = tx.subscribe();
    tx.send(10).unwrap();
    assert_eq!(rx1.recv().await.unwrap(), 10);
    drop(rx1);
    assert!(has_subscribers(&tx));

    assert_eq!(rx2.recv().await.unwrap(), 10);
    drop(rx2);
    assert!(!has_subscribers(&tx));

    let mut rx3 = tx.subscribe();
    tx.send(10).unwrap();
    assert_eq!(rx3.recv().await.unwrap(), 10);
    assert!(has_subscribers(&tx));
    drop(rx3);
    assert!(!has_subscribers(&tx));
}

#[tokio::test]
async fn subscription_stream_terminates_when_sender_dropped() {
    let (tx, rx) = tokio::sync::broadcast::channel::<u32>(8);
    let stream = subscription_stream(rx);

    tx.send(42).unwrap();
    drop(tx);

    let collected: Vec<u32> = stream.collect().await;
    assert_eq!(collected, vec![42]);
}

#[tokio::test]
async fn subscription_stream_skips_lagged_events_and_keeps_going() {
    // capacity=2 ring buffer; sending 5 values forces the slow receiver to lag.
    let (tx, rx) = tokio::sync::broadcast::channel::<u32>(2);
    let stream = subscription_stream(rx);

    for i in 0..5u32 {
        tx.send(i).unwrap();
    }
    drop(tx);

    // After Lagged, tokio's BroadcastStream catches up to the buffered window.
    // With capacity=2 the last two sends survive: 3 and 4.
    let collected: Vec<u32> = stream.collect().await;
    assert_eq!(collected, vec![3, 4]);
}
