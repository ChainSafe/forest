// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;

use libp2p::{
    core::{muxing::StreamMuxerBox, transport, upgrade},
    identity::{self, PublicKey},
    noise::{self, AuthenticKeypair, X25519Spec},
    yamux::YamuxConfig,
    PeerId, Transport,
};

pub type P2PTransport = (PeerId, StreamMuxerBox);

pub type BoxedP2PTransport = transport::Boxed<P2PTransport>;

#[derive(Clone)]
pub struct TransportBuilder {
    noise_keys: AuthenticKeypair<X25519Spec>,
    timeout: Duration,
    public_key: PublicKey,
    peer_id: PeerId,
}

impl TransportBuilder {
    /// Creates a new instance of [TransportBuilder] with random keypair and
    /// empty config
    pub fn new() -> Self {
        let keypair = identity::Keypair::generate_ed25519();
        Self::new_with_key(keypair)
    }

    /// Creates a new instance of [TransportBuilder] with given keypair and
    /// empty config
    pub fn new_with_key(keypair: identity::Keypair) -> Self {
        let public_key = keypair.public();
        let peer_id = PeerId::from(public_key.clone());
        let noise_keys = noise::Keypair::<noise::X25519Spec>::new()
            .into_authentic(&keypair)
            .expect("Signing libp2p-noise static DH keypair failed.");
        Self {
            noise_keys,
            timeout: Duration::from_secs(60),
            public_key,
            peer_id,
        }
    }

    /// Sets timeout duration for the [TransportBuilder] instance
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Builds libp2p transport
    pub fn build(self) -> Result<(BoxedP2PTransport, PublicKey, PeerId), std::io::Error> {
        let transport = {
            cfg_if::cfg_if! {
                if #[cfg(target_arch = "wasm32")] {
                    use libp2p::wasm_ext;

                    wasm_ext::ExtTransport::new(wasm_ext::ffi::websocket_transport())
                } else {
                    use libp2p::{dns, tcp, websocket};

                    let build_tcp = || tcp::tokio::Transport::new(tcp::Config::new().nodelay(true));
                    let build_dns_tcp = || dns::TokioDnsConfig::system(build_tcp());
                    let ws_dns_tcp = websocket::WsConfig::new(build_dns_tcp()?);
                    ws_dns_tcp.or_transport(build_dns_tcp()?)
                }
            }
        };

        Ok((
            transport
                .upgrade(upgrade::Version::V1)
                .authenticate(noise::NoiseConfig::xx(self.noise_keys).into_authenticated())
                .multiplex(YamuxConfig::default())
                .timeout(self.timeout)
                .boxed(),
            self.public_key,
            self.peer_id,
        ))
    }
}

impl Default for TransportBuilder {
    fn default() -> Self {
        Self::new()
    }
}
