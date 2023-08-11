// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use crate::libp2p_bitswap::*;
    use ahash::HashMap;
    use anyhow::Result;
    use futures::StreamExt;
    use libipld::{
        multihash::{self, MultihashDigest},
        Block, Cid,
    };
    use libp2p::{
        core,
        identity::Keypair,
        multiaddr::Protocol,
        noise,
        swarm::{SwarmBuilder, SwarmEvent},
        tcp, yamux, Multiaddr, PeerId, Swarm, Transport,
    };
    use parking_lot::RwLock;
    use rand::{rngs::OsRng, Rng};
    use tokio::{select, task::JoinSet};

    const TIMEOUT: Duration = Duration::from_secs(5);
    const LISTEN_ADDR: &str = "/ip4/127.0.0.1/tcp/0";
    const N_SERVER: usize = 10;

    #[tokio::test(flavor = "multi_thread")]
    async fn request_manager_e2e_test() {
        request_manager_e2e_test_mpl().await.unwrap();
    }

    async fn request_manager_e2e_test_mpl() -> Result<()> {
        let block_exist = new_random_block()?;
        let block_not_exist = new_random_block()?;

        // 1. Set up N servers, one of them have `block_exist` in its store
        let mut joinset = JoinSet::new();
        let mut server_addr_vec = vec![];
        let server_index_with_block = OsRng.gen_range(0..N_SERVER);
        for i in 0..N_SERVER {
            let (server, server_peer_id, server_peer_addr) = create_swarm().await?;
            println!("Server peer id: {server_peer_id}, address: {server_peer_addr}");
            server_addr_vec.push(server_peer_addr.with(Protocol::P2p(server_peer_id)));

            let server_store = TestStore::default();
            if i == server_index_with_block {
                server_store.insert(&block_exist)?;
            }
            joinset.spawn(run_swarm_loop(server, server_store));
        }

        let (mut client, client_peer_id, client_peer_addr) = create_swarm().await?;
        println!("Client peer id: {client_peer_id}, address: {client_peer_addr}");
        // 2. Connect the client to all servers
        for addr in server_addr_vec {
            client.dial(addr)?;
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
            );
            // Use a small timeout here
            tokio::task::spawn_blocking(move || request_rx.recv_timeout(Duration::from_secs(1)))
                .await?
                .expect_err(
                    "Should timeout, it does not fail fast (atm) in this case to reduce code complexity.",
                );
            assert!(!client_store.contains(block_not_exist.cid())?);
        }

        // 4. Get a block that exists on one of the servers
        {
            let (request_tx, request_rx) = flume::unbounded();
            client_request_manager.get_block(
                client_store.clone(),
                *block_exist.cid(),
                TIMEOUT,
                Some(request_tx),
            );
            let success =
                tokio::task::spawn_blocking(move || request_rx.recv_timeout(TIMEOUT)).await??;
            assert!(success);
            assert!(client_store.contains(block_exist.cid())?);
        }

        Ok(())
    }

    async fn create_swarm() -> Result<(Swarm<BitswapBehaviour>, PeerId, Multiaddr)> {
        let id_keys = Keypair::generate_ed25519();
        let peer_id = PeerId::from(id_keys.public());
        let transport = tcp::tokio::Transport::default()
            .upgrade(core::upgrade::Version::V1)
            .authenticate(noise::Config::new(&id_keys)?)
            .multiplex(yamux::Config::default())
            .timeout(TIMEOUT)
            .boxed();
        let behaviour = BitswapBehaviour::new(&["/test/ipfs/bitswap/1.0.0"], Default::default());
        let mut swarm = SwarmBuilder::with_tokio_executor(transport, behaviour, peer_id).build();
        swarm.listen_on(LISTEN_ADDR.parse()?)?;
        let peer_addr = loop {
            let event = swarm.select_next_some().await;
            if let SwarmEvent::NewListenAddr { address, .. } = event {
                break address;
            }
        };

        Ok((swarm, peer_id, peer_addr))
    }

    async fn run_swarm_loop(swarm: Swarm<BitswapBehaviour>, store: TestStore) -> Result<()> {
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
        swarm_event_opt: Option<
            SwarmEvent<BitswapBehaviourEvent, libp2p::swarm::THandlerErr<BitswapBehaviour>>,
        >,
        store: &impl BitswapStoreRead,
    ) -> Result<()> {
        if let Some(SwarmEvent::Behaviour(event)) = swarm_event_opt {
            let bitswap = &mut swarm.behaviour_mut();
            bitswap.handle_event(store, event)?;
        };

        Ok(())
    }

    fn new_random_block() -> Result<libipld::Block<libipld::DefaultParams>> {
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
