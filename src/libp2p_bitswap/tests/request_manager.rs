// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(test)]
mod tests {
    use crate::libp2p_bitswap::request_manager::{
        BitswapRequestManager, MAX_CONCURRENT_INBOUND_WANTLIST_SERVES, MAX_WANTLIST_ENTRIES_SERVED,
    };
    use crate::libp2p_bitswap::*;
    use crate::prelude::ShallowClone;
    use crate::utils::multihash::prelude::*;
    use crate::utils::rand::random_cid;
    use ahash::HashMap;
    use cid::Cid;
    use futures::StreamExt;
    use libp2p::{Multiaddr, PeerId, Swarm, multiaddr::Protocol, swarm::SwarmEvent};
    use libp2p_swarm_test::SwarmExt as _;
    use parking_lot::RwLock;
    use rand::Rng;
    use std::{sync::Arc, time::Duration};
    use tokio::{select, task::JoinSet};

    const TIMEOUT: Duration = Duration::from_secs(5);
    const N_SERVER: usize = 100;
    /// Window to confirm no *further* serve activity happens (a negative check).
    const QUIET: Duration = Duration::from_millis(500);

    #[tokio::test(flavor = "multi_thread")]
    async fn request_manager_e2e_test() {
        let block_exist = new_random_block().unwrap();
        let block_not_exist = new_random_block().unwrap();

        // 1. Set up N servers, one of them have `block_exist` in its store
        let mut joinset = JoinSet::new();
        let mut server_addr_vec = vec![];
        let server_index_with_block = crate::utils::rand::forest_rng().gen_range(0..N_SERVER);
        for i in 0..N_SERVER {
            let (server, server_peer_id, server_peer_addr) = create_swarm().await.unwrap();
            println!("Server peer id: {server_peer_id}, address: {server_peer_addr}");
            server_addr_vec.push(server_peer_addr.with(Protocol::P2p(server_peer_id)));

            let server_store = TestStore::default();
            if i == server_index_with_block {
                server_store.insert(&block_exist).unwrap();
            }
            joinset.spawn(run_swarm_loop(server, server_store));
        }

        let (mut client, client_peer_id, client_peer_addr) = create_swarm().await.unwrap();
        println!("Client peer id: {client_peer_id}, address: {client_peer_addr}");
        // 2. Connect the client to all servers
        for addr in server_addr_vec {
            client.dial(addr).unwrap();
        }

        let client_request_manager = client.behaviour().request_manager();
        let client_store = TestStore::default();
        joinset.spawn(run_swarm_loop(client, client_store.clone()));
        // Wait for 1s to establish connections
        tokio::time::sleep(Duration::from_secs(1)).await;

        // 3. Get a block that does not exist on any server
        {
            let (request_tx, request_rx) = flume::unbounded();
            client_request_manager.clone().get_block(
                client_store.clone(),
                *block_not_exist.cid(),
                TIMEOUT,
                Some(request_tx),
                None,
            );
            // Use a small timeout here
            tokio::task::spawn_blocking(move || request_rx.recv_timeout(Duration::from_secs(1)))
                .await.unwrap()
                .expect_err(
                    "Should timeout, it does not fail fast (atm) in this case to reduce code complexity.",
                );
            assert!(!client_store.contains(block_not_exist.cid()).unwrap());
        }

        // 4. Get a block that exists on one of the servers
        {
            let (request_tx, request_rx) = flume::unbounded();
            client_request_manager.get_block(
                client_store.clone(),
                *block_exist.cid(),
                TIMEOUT,
                Some(request_tx),
                Some(Arc::new(|_: PeerId| true)),
            );
            let success = tokio::task::spawn_blocking(move || request_rx.recv_timeout(TIMEOUT))
                .await
                .unwrap()
                .unwrap();
            assert!(success);
            assert!(client_store.contains(block_exist.cid()).unwrap());
        }
    }

    // Serving caps the entries handled per inbound wantlist, so a giant wantlist
    // can't turn into an unbounded burst of blockstore reads.
    #[tokio::test(flavor = "multi_thread")]
    async fn serve_caps_wantlist_entries_per_message() {
        let rm = Arc::new(BitswapRequestManager::default());
        // Empty store + `send_dont_have` makes every (missing) entry respond.
        let store = TestStore::default();

        let requests = (0..MAX_WANTLIST_ENTRIES_SERVED + 500)
            .map(|_| BitswapRequest::new_block(random_cid()).send_dont_have(true))
            .collect();
        rm.serve_inbound_requests(&store, PeerId::random(), requests);

        // Exactly the cap arrives...
        take_serve_responses(&rm, MAX_WANTLIST_ENTRIES_SERVED).await;
        // ...and serving stops there: nothing past the cap is produced.
        assert_no_serve_response(&rm, "serving must not exceed the per-message entry cap").await;
    }

    // Concurrent serves are bounded; wantlists past the cap are dropped, not
    // queued (each serve pins a blocking thread while it reads the store).
    #[tokio::test(flavor = "multi_thread")]
    async fn serve_drops_when_concurrency_saturated() {
        let cap = *MAX_CONCURRENT_INBOUND_WANTLIST_SERVES;
        let rm = Arc::new(BitswapRequestManager::default());
        let (entered_tx, entered_rx) = flume::unbounded();
        let (release_tx, release_rx) = flume::unbounded();
        let store = Arc::new(GatedStore {
            entered: entered_tx,
            release: release_rx,
        });
        let peer = PeerId::random();

        let one_block = || vec![BitswapRequest::new_block(random_cid()).send_dont_have(true)];

        // Saturate: `cap` serves each take a permit and block reading the store.
        for _ in 0..cap {
            rm.serve_inbound_requests(&store, peer, one_block());
        }
        for _ in 0..cap {
            entered_rx
                .recv_async()
                .await
                .expect("each of the first `cap` serves should run");
        }

        // The next serve finds no permit and must be dropped, not spawned.
        rm.serve_inbound_requests(&store, peer, one_block());
        assert!(
            tokio::time::timeout(QUIET, entered_rx.recv_async())
                .await
                .is_err(),
            "a serve past the concurrency cap should have been dropped",
        );

        // Releasing the pinned serves lets exactly `cap` of them respond.
        for _ in 0..cap {
            release_tx.send(()).unwrap();
        }
        take_serve_responses(&rm, cap).await;
        // The dropped serve must produce nothing — not be queued and served
        // once a permit frees.
        assert_no_serve_response(&rm, "a dropped serve must not be queued and served later").await;
    }

    // An empty wantlist (a message carrying only responses) is a no-op: it must
    // not take a serve permit or spawn a task.
    #[tokio::test(flavor = "multi_thread")]
    async fn serve_ignores_empty_wantlist() {
        let rm = Arc::new(BitswapRequestManager::default());
        let store = TestStore::default();
        let peer = PeerId::random();

        rm.serve_inbound_requests(&store, peer, vec![]);
        assert_no_serve_response(&rm, "an empty wantlist must not produce responses").await;

        // A real serve afterwards still works (the empty one wedged nothing).
        rm.serve_inbound_requests(
            &store,
            peer,
            vec![BitswapRequest::new_block(random_cid()).send_dont_have(true)],
        );
        take_serve_responses(&rm, 1).await;
    }

    /// Waits for exactly `expected` served responses, panicking if they don't
    /// all arrive within `TIMEOUT`. Deterministic: returns the moment the last
    /// expected response lands, with no reliance on an idle window.
    async fn take_serve_responses(rm: &BitswapRequestManager, expected: usize) {
        let stream = rm.outbound_serve_response_stream();
        tokio::time::timeout(TIMEOUT, stream.take(expected).count())
            .await
            .expect("timed out waiting for served responses");
    }

    /// Asserts no serve response arrives within the `QUIET` window (a negative
    /// check that serving produced nothing more).
    async fn assert_no_serve_response(rm: &BitswapRequestManager, msg: &str) {
        let mut stream = rm.outbound_serve_response_stream();
        assert!(
            tokio::time::timeout(QUIET, stream.next()).await.is_err(),
            "{msg}"
        );
    }

    /// A store whose reads block until released, used to pin serve permits.
    ///
    /// This depends on serving doing a synchronous per-entry blockstore read
    /// while holding its permit; if that changes (e.g. async reads, or skipping
    /// the store), the gate no longer pins permits and the drop test must adapt.
    struct GatedStore {
        entered: flume::Sender<()>,
        release: flume::Receiver<()>,
    }

    impl GatedStore {
        fn enter_and_wait(&self) {
            let _ = self.entered.send(());
            let _ = self.release.recv();
        }
    }

    impl BitswapStoreRead for GatedStore {
        fn contains(&self, _: &Cid) -> anyhow::Result<bool> {
            self.enter_and_wait();
            Ok(false)
        }

        fn get(&self, _: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
            self.enter_and_wait();
            Ok(None)
        }
    }

    async fn create_swarm() -> anyhow::Result<(Swarm<BitswapBehaviour>, PeerId, Multiaddr)> {
        let mut swarm = Swarm::new_ephemeral_tokio(|_| {
            BitswapBehaviour::new(&["/test/ipfs/bitswap/1.0.0"], Default::default())
        });
        let peer_id = *swarm.local_peer_id();
        let (peer_addr, _) = swarm.listen().with_memory_addr_external().await;

        Ok((swarm, peer_id, peer_addr))
    }

    async fn run_swarm_loop(
        swarm: Swarm<BitswapBehaviour>,
        store: TestStore,
    ) -> anyhow::Result<()> {
        let request_manager = swarm.behaviour().request_manager();
        let mut outbound_request_stream = request_manager.outbound_request_stream().fuse();
        let mut serve_response_stream = request_manager.outbound_serve_response_stream().fuse();
        let mut swarm_stream = swarm.fuse();

        loop {
            select! {
                // Hook libp2p swarm events
                swarm_event_opt = swarm_stream.next() => {
                    // `store` (an `Arc`) implements `BitswapStoreRead`
                    _ = handle_swarm_event(
                        swarm_stream.get_mut(),
                        swarm_event_opt,
                        &store,
                    );
                },
                request_opt = outbound_request_stream.next() => if let Some((peer, request)) = request_opt {
                    swarm_stream.get_mut().behaviour_mut().send_request(&peer, request);
                },
                serve_opt = serve_response_stream.next() => if let Some((peer, cid, response)) = serve_opt {
                    swarm_stream.get_mut().behaviour_mut().send_response(&peer, (cid, response));
                },
            }
        }
    }

    fn handle_swarm_event(
        swarm: &mut Swarm<BitswapBehaviour>,
        swarm_event_opt: Option<SwarmEvent<BitswapBehaviourEvent>>,
        store: &(impl BitswapStoreRead + ShallowClone + Send + Sync + 'static),
    ) -> anyhow::Result<()> {
        if let Some(SwarmEvent::Behaviour(event)) = swarm_event_opt {
            let bitswap = &mut swarm.behaviour_mut();
            bitswap.handle_event(store, event)?;
        };

        Ok(())
    }

    fn new_random_block()
    -> anyhow::Result<Block<<TestStoreInner as BitswapStoreReadWrite>::Hashes, 64>> {
        // 100KB
        let mut data = vec![0; 100 * 1024];
        crate::utils::rand::forest_rng().fill(&mut data[..]);
        let cid = Cid::new_v0(MultihashCode::Sha2_256.digest(data.as_slice()))?;
        Block::new(cid, data)
    }

    #[derive(Debug, Default)]
    struct TestStoreInner(RwLock<HashMap<Vec<u8>, Vec<u8>>>);

    type TestStore = Arc<TestStoreInner>;

    impl BitswapStoreRead for TestStoreInner {
        fn contains(&self, cid: &Cid) -> anyhow::Result<bool> {
            Ok(self.0.read().contains_key(&cid.to_bytes()))
        }

        fn get(&self, cid: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
            Ok(self.0.read().get(&cid.to_bytes()).cloned())
        }
    }

    impl BitswapStoreReadWrite for TestStoreInner {
        type Hashes = MultihashCode;

        fn insert(&self, block: &Block<Self::Hashes, 64>) -> anyhow::Result<()> {
            self.0
                .write()
                .insert(block.cid().to_bytes(), block.data().to_vec());
            Ok(())
        }
    }
}
