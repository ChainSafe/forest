// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::{
    multihash::{self, MultihashDigest},
    Cid,
};
use libp2p::{futures::StreamExt, Multiaddr, Swarm};
use rand::{rngs::OsRng, Rng};

use crate::*;

#[wasm_bindgen]
pub fn init_logger() -> Result<(), JsError> {
    static JS_LOGGER: JsExportableLogger = JsExportableLogger::new(log::Level::Debug);
    log::set_max_level(JS_LOGGER.max_level().to_level_filter());
    log::set_logger(&JS_LOGGER).map_err(err_to_js_error)
}

#[wasm_bindgen]
pub async fn connect(addr: &str, event_emitter: EventEmitter) -> Result<Connection, JsError> {
    let addr = addr.parse().map_err(err_to_js_error)?;
    connect_async(addr, event_emitter)
        .await
        .map_err(err_to_js_error)
}

async fn connect_async(addr: Multiaddr, event_emitter: EventEmitter) -> anyhow::Result<Connection> {
    let (transport, _, local_peer_id) = TransportBuilder::default().build()?;
    log::info!("[WASM] Connecting to forest daemon via ws at {addr} ... ",);
    let mut swarm = {
        let behaviour = DemoBehaviour::default();
        Swarm::with_wasm_executor(transport, behaviour, local_peer_id)
    };
    swarm.dial(addr)?;
    let peer_id = loop {
        match swarm.select_next_some().await {
            libp2p::swarm::SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                break peer_id;
            }
            _ => {}
        }
    };
    Ok(ConnectionImpl::new(swarm, peer_id, event_emitter).into())
}

#[wasm_bindgen]
pub fn random_cid() -> Result<String, JsError> {
    let mut data = [0_u8; 16];
    OsRng.fill(&mut data);
    let cid = Cid::new_v0(multihash::Code::Sha2_256.digest(data.as_slice())).map_err(map_js_err)?;
    Ok(cid.to_string())
}
