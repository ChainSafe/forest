// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(test)]
mod tests {
    use std::{process::Command, time::Duration};

    use crate::libp2p_bitswap::{
        BitswapBehaviour, BitswapBehaviourEvent, BitswapMessage, BitswapRequest, BitswapResponse,
    };
    use anyhow::{Context, Result};
    use libipld::{
        multihash::{self, MultihashDigest},
        Cid,
    };
    use libp2p::{
        core,
        futures::StreamExt,
        identity, noise, request_response,
        swarm::{SwarmBuilder, SwarmEvent},
        tcp, yamux, PeerId, Transport,
    };

    const TIMEOUT: Duration = Duration::from_secs(60);
    const LISTEN_ADDR: &str = "/ip4/127.0.0.1/tcp/0";
    const GO_APP_DIR: &str = "src/libp2p_bitswap/tests/go-app";

    #[tokio::test(flavor = "multi_thread")]
    async fn bitswap_go_compat_test() {
        bitswap_go_compat_test_impl().await.unwrap()
    }

    async fn bitswap_go_compat_test_impl() -> Result<()> {
        let id_keys = identity::Keypair::generate_ed25519();
        let peer_id = PeerId::from(id_keys.public());
        let transport = tcp::tokio::Transport::default()
            .upgrade(core::upgrade::Version::V1)
            .authenticate(noise::Config::new(&id_keys)?)
            .multiplex(yamux::Config::default())
            .timeout(TIMEOUT)
            .boxed();
        let behaviour = BitswapBehaviour::new(&["/test/ipfs/bitswap/1.2.0"], Default::default());
        let mut swarm = SwarmBuilder::with_tokio_executor(transport, behaviour, peer_id).build();
        swarm.listen_on(LISTEN_ADDR.parse()?)?;
        let expected_inbound_request_cid_str = "bitswap_request_from_go";
        let expected_inbound_request_cid = Cid::new_v0(
            multihash::Code::Sha2_256.digest(expected_inbound_request_cid_str.as_bytes()),
        )?;
        let outbound_request_cid =
            Cid::new_v0(multihash::Code::Sha2_256.digest(b"bitswap_request_from_rust"))?;
        let (local_addr_tx, local_addr_rx) = flume::unbounded();
        let (inbound_request_tx, inbound_request_rx) = flume::unbounded();
        let (inbound_response_tx, inbound_response_rx) = flume::unbounded();
        tokio::spawn(async move {
            loop {
                // Swarm event loop
                match swarm.select_next_some().await {
                    SwarmEvent::Behaviour(BitswapBehaviourEvent::Message { peer, message }) => {
                        let bitswap = &mut swarm.behaviour_mut();
                        match message {
                            request_response::Message::Request {
                                request_id: _,
                                request,
                                channel,
                            } => {
                                // Close the stream immediately, `go-bitswap` does not read
                                // response(s) from this stream
                                // so they will be sent over another stream
                                bitswap.inner_mut().send_response(channel, ()).unwrap();
                                for message in request {
                                    match message {
                                        BitswapMessage::Request(r) => {
                                            if r.cancel {
                                                continue;
                                            }

                                            // Send a response to the go app
                                            bitswap.send_response(
                                                &peer,
                                                (r.cid, BitswapResponse::Have(false)),
                                            );
                                            // 1. Get an inbound request from go app
                                            if r.cid == expected_inbound_request_cid {
                                                inbound_request_tx.send_async(peer).await.unwrap();
                                                // Send a request to the go app
                                                bitswap.send_request(
                                                    &peer,
                                                    BitswapRequest::new_have(outbound_request_cid)
                                                        .send_dont_have(true),
                                                );
                                            }
                                        }
                                        BitswapMessage::Response(cid, ..) => {
                                            // 2. Check inbound response
                                            if cid == outbound_request_cid {
                                                inbound_response_tx.send_async(()).await.unwrap();
                                            }
                                        }
                                    }
                                }
                            }
                            request_response::Message::Response { .. } => {}
                        }
                    }
                    SwarmEvent::NewListenAddr {
                        listener_id: _,
                        address,
                    } => {
                        local_addr_tx.send_async(address).await.unwrap();
                    }
                    _ => {}
                }
            }
        });

        let (cancellation_tx, cancellation_rx) = flume::bounded(1);

        let listen_addr = tokio::time::timeout(TIMEOUT, local_addr_rx.recv_async()).await??;
        // Prepare `go-bitswap` test app
        prepare_go_bitswap()?;
        // Build and run `go-bitswap` test app
        std::thread::spawn(move || {
            let addr = format!("{listen_addr}/p2p/{peer_id}");
            run_go_bitswap(cancellation_rx, addr, expected_inbound_request_cid_str)
        });
        let cancellation_tx_cloned = cancellation_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(TIMEOUT).await;
            println!("cancelling");
            cancellation_tx_cloned.send_async(()).await.unwrap();
        });

        // 1. Receive request from `go-bitswap`
        tokio::time::timeout(TIMEOUT, inbound_request_rx.recv_async()).await??;
        println!("Received request from go-bitswap test app");
        // 2. Receive response from `go-bitswap`
        tokio::time::timeout(TIMEOUT, inbound_response_rx.recv_async()).await??;
        println!("Received response from go-bitswap test app");
        cancellation_tx.send_async(()).await.unwrap();

        Ok(())
    }

    fn prepare_go_bitswap() -> Result<()> {
        const ERROR_CONTEXT: &str = "Fail to compile `go-bitswap` test app, make sure you have `Go1.20.x` compiler installed and available in $PATH. For details refer to instructions at <https://go.dev/doc/install>";
        Command::new("go")
            .args(["mod", "vendor"])
            .current_dir(GO_APP_DIR)
            .spawn()
            .context(ERROR_CONTEXT)?
            .wait()
            .context(ERROR_CONTEXT)?;
        Ok(())
    }

    fn run_go_bitswap(
        cancellation_rx: flume::Receiver<()>,
        addr: impl AsRef<str>,
        cid: impl AsRef<str>,
    ) -> Result<bool> {
        let mut app = Command::new("go")
            .args(["run", ".", "--addr", addr.as_ref(), "--cid", cid.as_ref()])
            .current_dir(GO_APP_DIR)
            .spawn()?;
        loop {
            if cancellation_rx
                .recv_timeout(Duration::from_millis(100))
                .is_ok()
            {
                app.kill()?;
                return Ok(false);
            }

            if let Some(status) = app.try_wait()? {
                return Ok(status.success());
            }
        }
    }
}
