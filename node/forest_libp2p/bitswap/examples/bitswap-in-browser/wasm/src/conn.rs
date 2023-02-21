// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt::Display;

use cid::Cid;
use forest_libp2p_bitswap::*;
use libp2p::{
    futures::StreamExt, request_response::RequestResponseMessage, swarm::SwarmEvent, PeerId, Swarm,
};
use serde::{Deserialize, Serialize};
use tokio::{select, sync::Mutex};

use crate::*;

#[wasm_bindgen]
pub struct Connection {
    ptr: *const ConnectionImpl,
}

impl Connection {
    fn inner(&self) -> &ConnectionImpl {
        unsafe { &*self.ptr }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.free()
    }
}

#[wasm_bindgen]
impl Connection {
    #[wasm_bindgen]
    pub async fn run(&self) -> Result<(), JsError> {
        self.inner().run().await.map_err(map_js_err)
    }

    #[wasm_bindgen]
    pub fn bitswap_get(&self, cid_str: &str) -> Result<(), JsError> {
        self.inner()
            .send_request(cid_str.try_into().map_err(map_js_err)?)
            .map_err(map_js_err)
    }

    #[wasm_bindgen]
    pub fn free(&self) {
        unsafe {
            let inner = Box::from_raw(self.ptr as *mut ConnectionImpl);
            drop(inner);
        }
    }
}

pub struct ConnectionImpl {
    pub swarm: Mutex<Swarm<DemoBehaviour>>,
    pub target: PeerId,
    pub event_emitter: EventEmitter,
    pub tx: flume::Sender<Cid>,
    pub rx: flume::Receiver<Cid>,
}

impl From<ConnectionImpl> for Connection {
    fn from(value: ConnectionImpl) -> Self {
        Self {
            ptr: Box::into_raw(Box::new(value)),
        }
    }
}

impl ConnectionImpl {
    pub fn new(swarm: Swarm<DemoBehaviour>, target: PeerId, event_emitter: EventEmitter) -> Self {
        let (tx, rx) = flume::unbounded();
        Self {
            swarm: Mutex::new(swarm),
            target,
            event_emitter,
            tx,
            rx,
        }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let mut swarm = self.swarm.try_lock()?;
        loop {
            select! {
                event = swarm.select_next_some() => {
                    if let Err(err) = handle_swarm_event(&mut swarm, &self.event_emitter, event) {
                        log::error!("{err}");
                    }
                }
                request = self.rx.recv_async() => if let Ok(cid) = request {
                    self.bitswap_get(&mut swarm, cid);
                }
            }
        }
    }

    pub fn send_request(&self, cid: Cid) -> anyhow::Result<()> {
        self.tx.send(cid)?;
        Ok(())
    }

    fn bitswap_get(&self, swarm: &mut Swarm<DemoBehaviour>, cid: Cid) {
        swarm.behaviour_mut().bitswap.send_request(
            &self.target,
            BitswapRequest::new_block(cid).send_dont_have(true),
        );
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BitswapResponseJson {
    cid: String,
    response: String,
}

fn handle_swarm_event<Err: Display>(
    swarm: &mut Swarm<DemoBehaviour>,
    event_emitter: &EventEmitter,
    event: SwarmEvent<DemoBehaviourEvent, Err>,
) -> anyhow::Result<()> {
    match event {
        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
            log::info!("[WASM] Connected to {}", peer_id);
        }
        SwarmEvent::Behaviour(DemoBehaviourEvent::Bitswap(e)) => match e {
            BitswapBehaviourEvent::Message { peer: _, message } => {
                log::info!("{message:?}");
                match message {
                    RequestResponseMessage::Request {
                        request, channel, ..
                    } => {
                        _ = swarm
                            .behaviour_mut()
                            .bitswap
                            .inner_mut()
                            .send_response(channel, ());
                        for message in request {
                            match message {
                                BitswapMessage::Response(cid, response) => {
                                    log::info!(
                                        "bitswap response {cid}:\n{}",
                                        serde_json::to_string_pretty(&response)?
                                    );
                                    event_emitter.emit_str(
                                        "bitswap",
                                        &serde_json::to_string(&BitswapResponseJson {
                                            cid: cid.to_string(),
                                            response: serde_json::to_string(&response)?,
                                        })?,
                                    )
                                }
                                BitswapMessage::Request(request) => {
                                    log::info!(
                                        "bitswap request:\n{}",
                                        serde_json::to_string_pretty(&request)?,
                                    );
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        },
        _ => {}
    }
    Ok(())
}
