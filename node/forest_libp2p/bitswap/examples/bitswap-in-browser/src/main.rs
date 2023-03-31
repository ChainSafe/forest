// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{fmt::Display, sync::Arc};

use ahash::HashMap;
use bitswap_in_browser_lib::*;
use cid::{
    multihash::{self, MultihashDigest},
    Cid,
};
use forest_libp2p_bitswap::{BitswapStoreRead, BitswapStoreReadWrite};
use libipld::Block;
use libp2p::{
    futures::StreamExt,
    multiaddr::Protocol,
    swarm::{SwarmBuilder, SwarmEvent},
    Swarm,
};
use parking_lot::RwLock;
use rand::{rngs::OsRng, Rng};
use tokio::select;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    let (transport, _, local_peer_id) = TransportBuilder::new().build()?;
    let behaviour = DemoBehaviour::default();
    let bitswap_request_manager = behaviour.bitswap.request_manager();
    let mut swarm = SwarmBuilder::with_tokio_executor(transport, behaviour, local_peer_id).build();
    swarm.listen_on("/ip4/127.0.0.1/tcp/0/ws".parse()?)?;
    let local_peer_addr = loop {
        let event = swarm.select_next_some().await;
        if let SwarmEvent::NewListenAddr { address, .. } = event {
            break address;
        }
    };
    log::info!(
        "Address: {}",
        local_peer_addr
            .clone()
            .with(Protocol::P2p(local_peer_id.into()))
    );

    let store = MemoryStore::default();
    for _ in 0..5 {
        let block = {
            let mut data = vec![0; 16];
            OsRng.fill(&mut data[..]);
            let cid = Cid::new_v0(multihash::Code::Sha2_256.digest(data.as_slice()))?;
            Block::new(cid, data)
        }?;
        store.insert(&block)?;
        log::info!("Inserting block {}", block.cid());
    }
    loop {
        select! {
            // Hook libp2p swarm events
            swarm_event = swarm.select_next_some() => {
                handle_swarm_event(
                    &mut swarm,
                    swarm_event,
                    store.as_ref(),
                );
            },
            request = bitswap_request_manager.outbound_request_rx().recv_async() => if let Ok((peer, request)) = request {
                swarm.behaviour_mut().bitswap.send_request(&peer, request);
            },
        }
    }
}

fn handle_swarm_event<Err: Display>(
    swarm: &mut Swarm<DemoBehaviour>,
    swarm_event: SwarmEvent<DemoBehaviourEvent, Err>,
    store: &impl BitswapStoreRead,
) {
    if let SwarmEvent::Behaviour(DemoBehaviourEvent::Bitswap(event)) = swarm_event {
        if let Err(err) = swarm.behaviour_mut().bitswap.handle_event(store, event) {
            log::error!("{err}");
        }
    }
}

#[derive(Debug, Default)]
struct MemoryStoreInner(RwLock<HashMap<Vec<u8>, Vec<u8>>>);

type MemoryStore = Arc<MemoryStoreInner>;

impl BitswapStoreRead for MemoryStoreInner {
    fn contains(&self, cid: &libipld::Cid) -> anyhow::Result<bool> {
        Ok(self.0.read().contains_key(&cid.to_bytes()))
    }

    fn get(&self, cid: &libipld::Cid) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.0.read().get(&cid.to_bytes()).cloned())
    }
}

impl BitswapStoreReadWrite for MemoryStoreInner {
    type Params = libipld::DefaultParams;

    fn insert(&self, block: &libipld::Block<Self::Params>) -> anyhow::Result<()> {
        self.0
            .write()
            .insert(block.cid().to_bytes(), block.data().to_vec());
        Ok(())
    }
}
