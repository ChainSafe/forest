// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

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
