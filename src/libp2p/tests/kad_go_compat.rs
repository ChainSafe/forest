// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::Context as _;
use futures::StreamExt as _;
use libp2p::{
    identify, identity, kad, noise, swarm::SwarmEvent, tcp, yamux, Multiaddr, StreamProtocol,
    Swarm, SwarmBuilder,
};
use libp2p_swarm_test::SwarmExt as _;
use std::{process::Command, time::Duration};

use crate::libp2p::discovery::new_kademlia;

const TIMEOUT: Duration = Duration::from_secs(600);
const LISTEN_ADDR: &str = "/ip4/127.0.0.1/tcp/0";
const GO_APP_DIR: &str = "src/libp2p/tests/go-kad";

type SwarmType = Swarm<TestBehaviour>;

#[tokio::test(flavor = "multi_thread")]
async fn kad_go_compat_test() -> anyhow::Result<()> {
    prepare_go_app()?;
    let (cancellation_tx, cancellation_rx) = flume::bounded(1);
    tokio::spawn({
        let cancellation_tx = cancellation_tx.clone();
        async move {
            tokio::time::sleep(TIMEOUT).await;
            println!("timed out, cancelling");
            cancellation_tx.send_async(()).await.unwrap();
        }
    });
    let (mut swarm1, addr1) = create_node().await?;
    let (swarm2, addr2) = create_node().await?;
    swarm1
        .behaviour_mut()
        .kad
        .add_address(swarm2.local_peer_id(), addr2);

    tokio::spawn(swarm1.loop_on_next());
    tokio::spawn(swarm2.loop_on_next());

    assert!(run_go_app(cancellation_rx, &addr1)?);
    Ok(())
}

fn prepare_go_app() -> anyhow::Result<()> {
    const ERROR_CONTEXT: &str = "Fail to compile `go-kad` test app, make sure you have `Go1.21.x` compiler installed and available in $PATH. For details refer to instructions at <https://go.dev/doc/install>";
    Command::new("go")
        .args(["mod", "vendor"])
        .current_dir(GO_APP_DIR)
        .spawn()
        .context(ERROR_CONTEXT)?
        .wait()
        .context(ERROR_CONTEXT)?;
    Ok(())
}

fn run_go_app(cancellation_rx: flume::Receiver<()>, addr: &Multiaddr) -> anyhow::Result<bool> {
    let mut app = Command::new("go")
        .args(["run", ".", "--addr", addr.to_string().as_str()])
        .current_dir(GO_APP_DIR)
        .env("GOLOG_LOG_LEVEL", "info,dht=debug")
        .spawn()?;
    loop {
        if cancellation_rx
            .recv_timeout(Duration::from_millis(100))
            .is_ok()
        {
            app.kill()?;
            anyhow::bail!("Cancelled");
        }

        if let Some(status) = app.try_wait()? {
            return Ok(status.success());
        }
    }
}

async fn create_node() -> anyhow::Result<(SwarmType, Multiaddr)> {
    let mut swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|keypair| TestBehaviour::new(keypair.public()))?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(TIMEOUT))
        .build();
    let local_peer_id = *swarm.local_peer_id();
    swarm.listen_on(LISTEN_ADDR.parse()?)?;
    let listen_addr = {
        loop {
            if let SwarmEvent::NewListenAddr {
                listener_id: _,
                address,
            } = swarm.select_next_some().await
            {
                break address;
            }
        }
    };

    Ok((swarm, listen_addr.with_p2p(local_peer_id).unwrap()))
}

#[derive(libp2p::swarm::NetworkBehaviour)]
#[behaviour(prelude = "libp2p::swarm::derive_prelude")]
struct TestBehaviour {
    kad: kad::Behaviour<kad::store::MemoryStore>,
    identify: identify::Behaviour,
}

impl TestBehaviour {
    fn new(local_public_key: identity::PublicKey) -> Self {
        let kad_peer_id = local_public_key.to_peer_id();
        let kad = new_kademlia(kad_peer_id, vec![StreamProtocol::new("/kadtest/kad/1.0.0")]);
        let identify = identify::Behaviour::new(
            identify::Config::new(Default::default(), local_public_key)
                .with_push_listen_addr_updates(true),
        );
        Self { kad, identify }
    }
}
