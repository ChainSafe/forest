// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use crate::libp2p_bitswap::*;
    use ahash::HashMap;
    use futures::StreamExt;
    use libipld::{
        multihash::{self, MultihashDigest},
        Block, Cid,
    };
    use libp2p::{multiaddr::Protocol, swarm::SwarmEvent, Multiaddr, PeerId, Swarm};
    use libp2p_swarm_test::SwarmExt;
    use parking_lot::RwLock;
    use rand::{rngs::OsRng, Rng};
    use tokio::{select, task::JoinSet};

    const TIMEOUT: Duration = Duration::from_secs(5);
    const N_SERVER: usize = 10;

    #[tokio::test(flavor = "multi_thread")]
    async fn request_manager_e2e_test() {
        let block_exist = new_random_block().unwrap();
        let block_not_exist = new_random_block().unwrap();

        // 1. Set up N servers, one of them have `block_exist` in its store
        let mut joinset = JoinSet::new();
        let mut server_addr_vec = vec![];
        let server_index_with_block = OsRng.gen_range(0..N_SERVER);
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

    async fn create_swarm() -> anyhow::Result<(Swarm<BitswapBehaviour>, PeerId, Multiaddr)> {
        let mut swarm = Swarm::new_ephemeral(|_| {
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
        let mut outbound_request_rx_stream = request_manager.outbound_request_rx().stream().fuse();
        let mut swarm_stream = swarm.fuse();

        loop {
            select! {
                // Hook libp2p swarm events
                swarm_event_opt = swarm_stream.next() => {
                    // `store` implements `BitswapStoreRead`
                    _ = handle_swarm_event(
                        swarm_stream.get_mut(),
                        swarm_event_opt,
                        store.as_ref(),
                    );
                },
                request_opt = outbound_request_rx_stream.next() => if let Some((peer, request)) = request_opt {
                    swarm_stream.get_mut().behaviour_mut().send_request(&peer, request);
                },
            }
        }
    }

    fn handle_swarm_event(
        swarm: &mut Swarm<BitswapBehaviour>,
        swarm_event_opt: Option<SwarmEvent<BitswapBehaviourEvent>>,
        store: &impl BitswapStoreRead,
    ) -> anyhow::Result<()> {
        if let Some(SwarmEvent::Behaviour(event)) = swarm_event_opt {
            let bitswap = &mut swarm.behaviour_mut();
            bitswap.handle_event(store, event)?;
        };

        Ok(())
    }

    fn new_random_block() -> anyhow::Result<libipld::Block<libipld::DefaultParams>> {
        // 100KB
        let mut data = vec![0; 100 * 1024];
        OsRng.fill(&mut data[..]);
        let cid = Cid::new_v0(multihash::Code::Sha2_256.digest(data.as_slice()))?;
        Block::new(cid, data)
    }

    #[derive(Debug, Default)]
    struct TestStoreInner(RwLock<HashMap<Vec<u8>, Vec<u8>>>);

    type TestStore = Arc<TestStoreInner>;

    impl BitswapStoreRead for TestStoreInner {
        fn contains(&self, cid: &libipld::Cid) -> anyhow::Result<bool> {
            Ok(self.0.read().contains_key(&cid.to_bytes()))
        }

        fn get(&self, cid: &libipld::Cid) -> anyhow::Result<Option<Vec<u8>>> {
            Ok(self.0.read().get(&cid.to_bytes()).cloned())
        }
    }

    impl BitswapStoreReadWrite for TestStoreInner {
        type Params = libipld::DefaultParams;

        fn insert(&self, block: &libipld::Block<Self::Params>) -> anyhow::Result<()> {
            self.0
                .write()
                .insert(block.cid().to_bytes(), block.data().to_vec());
            Ok(())
        }
    }
}
