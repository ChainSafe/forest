// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{error::Error, time::Duration};

use futures::stream::StreamExt;
use libp2p::{SwarmBuilder, core::Multiaddr, noise, ping, swarm::SwarmEvent, tcp, yamux};

pub async fn p2p_ping(addr: Multiaddr) -> Result<Duration, ping::Failure> {
    let mut swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )
        .map_err(map_failure)?
        .with_quic()
        .with_dns()
        .map_err(map_failure)?
        .with_behaviour(|_keypair| ping::Behaviour::default())
        .map_err(map_failure)?
        .build();
    swarm.dial(addr).map_err(map_failure)?;
    let mut conn_established_in = None;
    loop {
        match tokio::time::timeout(Duration::from_secs(5), swarm.select_next_some()).await {
            Ok(SwarmEvent::Behaviour(ping_event)) => return ping_event.result,
            Ok(SwarmEvent::ConnectionEstablished { established_in, .. }) => {
                // The connection might be dropped before receiving ping event
                // due to incomplete protocols the peer supports,
                // use connection establishing duration instead in this case.
                conn_established_in = Some(established_in);
            }
            Ok(e) => {
                tracing::trace!("{e:?}");
            }
            Err(_e) => {
                return conn_established_in.ok_or(ping::Failure::Timeout);
            }
        }
    }
}

fn map_failure<E: Error + Send + Sync + 'static>(error: E) -> ping::Failure {
    ping::Failure::Other {
        error: Box::new(error),
    }
}
