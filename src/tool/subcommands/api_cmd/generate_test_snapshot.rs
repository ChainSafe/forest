// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::{
    blocks::TipsetKey,
    chain::ChainStore,
    chain_sync::{network_context::SyncNetworkContext, SyncStage},
    daemon::db_util::load_all_forest_cars,
    db::{
        db_engine::open_db, parity_db::ParityDb, EthMappingsStore, HeaviestTipsetKeyProvider,
        MemoryDB, SettingsStore, SettingsStoreExt, CAR_DB_DIR_NAME,
    },
    genesis::read_genesis_header,
    libp2p::{NetworkMessage, PeerManager},
    libp2p_bitswap::{BitswapStoreRead, BitswapStoreReadWrite, Block64},
    message_pool::{MessagePool, MpoolRpcProvider},
    networks::ChainConfig,
    shim::address::CurrentNetwork,
    state_manager::StateManager,
    KeyStore, KeyStoreConfig,
};
use api_compare_tests::TestDump;
use fvm_shared4::address::Network;
use openrpc_types::ParamStructure;
use parking_lot::RwLock;
use rpc::{eth::filter::EthEventHandler, RPCState, RpcMethod as _};
use tokio::{sync::mpsc, task::JoinSet};

pub async fn run_test_with_dump(
    test_dump: &TestDump,
    db: Arc<ReadOpsTrackingStore<ManyCar<ParityDb>>>,
    chain: &NetworkChain,
    allow_response_mismatch: bool,
) -> anyhow::Result<()> {
    if chain.is_testnet() {
        CurrentNetwork::set_global(Network::Testnet);
    }
    let mut run = false;
    let chain_config = Arc::new(ChainConfig::from_chain(chain));
    let (ctx, _, _) = ctx(db, chain_config).await?;
    let params_raw = Some(serde_json::to_string(&test_dump.request.params)?);
    macro_rules! run_test {
        ($ty:ty) => {
            if test_dump.request.method_name.as_ref() == <$ty>::NAME {
                let params = <$ty>::parse_params(params_raw.clone(), ParamStructure::Either)?;
                let result = <$ty>::handle(ctx.clone(), params).await?;
                anyhow::ensure!(
                    allow_response_mismatch
                        || test_dump.forest_response == Ok(result.into_lotus_json_value()?),
                    "Response mismatch between Forest and Lotus"
                );
                run = true;
            }
        };
    }
    crate::for_each_rpc_method!(run_test);
    anyhow::ensure!(run, "RPC method not found");
    Ok(())
}

pub(super) fn load_db(
    db_root: &Path,
) -> anyhow::Result<Arc<ReadOpsTrackingStore<ManyCar<ParityDb>>>> {
    let db_writer = open_db(db_root.into(), Default::default())?;
    let db = ManyCar::new(db_writer);
    let forest_car_db_dir = db_root.join(CAR_DB_DIR_NAME);
    load_all_forest_cars(&db, &forest_car_db_dir)?;
    Ok(Arc::new(ReadOpsTrackingStore::new(db)))
}

pub(super) fn build_index(db: Arc<ReadOpsTrackingStore<ManyCar<ParityDb>>>) -> Option<Index> {
    let mut index = Index::default();
    let reader = db.tracker.eth_mappings_db.read();
    for (k, v) in reader.iter() {
        index.eth_mappings.insert(k.to_string(), Payload(v.clone()));
    }
    if index == Index::default() {
        None
    } else {
        Some(index)
    }
}

async fn ctx(
    db: Arc<ReadOpsTrackingStore<ManyCar<ParityDb>>>,
    chain_config: Arc<ChainConfig>,
) -> anyhow::Result<(
    Arc<RPCState<ReadOpsTrackingStore<ManyCar<ParityDb>>>>,
    flume::Receiver<NetworkMessage>,
    tokio::sync::mpsc::Receiver<()>,
)> {
    let (network_send, network_rx) = flume::bounded(5);
    let (tipset_send, _) = flume::bounded(5);
    let genesis_header =
        read_genesis_header(None, chain_config.genesis_bytes(&db).await?.as_deref(), &db).await?;

    let chain_store = Arc::new(
        ChainStore::new(
            db.clone(),
            db.clone(),
            db,
            chain_config.clone(),
            genesis_header.clone(),
        )
        .unwrap(),
    );

    let state_manager = Arc::new(StateManager::new(chain_store.clone(), chain_config).unwrap());
    let network_name = state_manager.get_network_name_from_genesis()?;
    let message_pool = MessagePool::new(
        MpoolRpcProvider::new(chain_store.publisher().clone(), state_manager.clone()),
        network_name.clone(),
        network_send.clone(),
        Default::default(),
        state_manager.chain_config().clone(),
        &mut JoinSet::new(),
    )?;

    let peer_manager = Arc::new(PeerManager::default());
    let sync_network_context =
        SyncNetworkContext::new(network_send, peer_manager, state_manager.blockstore_owned());
    let (shutdown, shutdown_recv) = mpsc::channel(1);
    let rpc_state = Arc::new(RPCState {
        state_manager,
        keystore: Arc::new(tokio::sync::RwLock::new(KeyStore::new(
            KeyStoreConfig::Memory,
        )?)),
        mpool: Arc::new(message_pool),
        bad_blocks: Default::default(),
        sync_states: Arc::new(RwLock::new(nunny::vec![Default::default()])),
        eth_event_handler: Arc::new(EthEventHandler::new()),
        sync_network_context,
        network_name,
        start_time: chrono::Utc::now(),
        shutdown,
        tipset_send,
    });
    rpc_state
        .sync_states
        .write()
        .first_mut()
        .set_stage(SyncStage::Idle);
    Ok((rpc_state, network_rx, shutdown_recv))
}

/// A [`Blockstore`] wrapper that tracks read operations to the inner [`Blockstore`] with an [`MemoryDB`]
pub struct ReadOpsTrackingStore<T> {
    inner: T,
    pub tracker: Arc<MemoryDB>,
}

impl<T> ReadOpsTrackingStore<T>
where
    T: Blockstore + SettingsStore + HeaviestTipsetKeyProvider,
{
    fn is_chain_head_tracked(&self) -> anyhow::Result<bool> {
        SettingsStore::exists(&self.tracker, crate::db::setting_keys::HEAD_KEY)
    }

    pub fn ensure_chain_head_is_tracked(&self) -> anyhow::Result<()> {
        if !self.is_chain_head_tracked()? {
            SettingsStoreExt::write_obj(
                &self.tracker,
                crate::db::setting_keys::HEAD_KEY,
                &self.inner.heaviest_tipset_key()?,
            )?;
        }

        Ok(())
    }
}

impl<T> ReadOpsTrackingStore<T>
where
    T: Blockstore + SettingsStore,
{
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            tracker: Arc::new(Default::default()),
        }
    }

    pub async fn export_forest_car<W: tokio::io::AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
    ) -> anyhow::Result<()> {
        self.tracker.export_forest_car(writer).await
    }
}

impl<T: HeaviestTipsetKeyProvider> HeaviestTipsetKeyProvider for ReadOpsTrackingStore<T> {
    fn heaviest_tipset_key(&self) -> anyhow::Result<TipsetKey> {
        self.inner.heaviest_tipset_key()
    }

    fn set_heaviest_tipset_key(&self, tsk: &TipsetKey) -> anyhow::Result<()> {
        self.inner.set_heaviest_tipset_key(tsk)
    }
}

impl<T: Blockstore> Blockstore for ReadOpsTrackingStore<T> {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        let result = self.inner.get(k)?;
        if let Some(v) = &result {
            self.tracker.put_keyed(k, v.as_slice())?;
        }
        Ok(result)
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        self.inner.put_keyed(k, block)
    }
}

impl<T: SettingsStore> SettingsStore for ReadOpsTrackingStore<T> {
    fn read_bin(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let result = self.inner.read_bin(key)?;
        if let Some(v) = &result {
            SettingsStore::write_bin(&self.tracker, key, v.as_slice())?;
        }
        Ok(result)
    }

    fn write_bin(&self, key: &str, value: &[u8]) -> anyhow::Result<()> {
        self.inner.write_bin(key, value)
    }

    fn exists(&self, key: &str) -> anyhow::Result<bool> {
        let result = self.inner.read_bin(key)?;
        if let Some(v) = &result {
            SettingsStore::write_bin(&self.tracker, key, v.as_slice())?;
        }
        Ok(result.is_some())
    }

    fn setting_keys(&self) -> anyhow::Result<Vec<String>> {
        self.inner.setting_keys()
    }
}

impl<T: BitswapStoreRead> BitswapStoreRead for ReadOpsTrackingStore<T> {
    fn contains(&self, cid: &Cid) -> anyhow::Result<bool> {
        let result = self.inner.get(cid)?;
        if let Some(v) = &result {
            Blockstore::put_keyed(&self.tracker, cid, v.as_slice())?;
        }
        Ok(result.is_some())
    }

    fn get(&self, cid: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        let result = self.inner.get(cid)?;
        if let Some(v) = &result {
            Blockstore::put_keyed(&self.tracker, cid, v.as_slice())?;
        }
        Ok(result)
    }
}

impl<T: BitswapStoreReadWrite> BitswapStoreReadWrite for ReadOpsTrackingStore<T> {
    type Hashes = <T as BitswapStoreReadWrite>::Hashes;

    fn insert(&self, block: &Block64<Self::Hashes>) -> anyhow::Result<()> {
        self.inner.insert(block)
    }
}

impl<T: EthMappingsStore> EthMappingsStore for ReadOpsTrackingStore<T> {
    fn read_bin(&self, key: &EthHash) -> anyhow::Result<Option<Vec<u8>>> {
        let result = self.inner.read_bin(key)?;
        if let Some(v) = &result {
            EthMappingsStore::write_bin(&self.tracker, key, v.as_slice())?;
        }
        self.inner.read_bin(key)
    }

    fn write_bin(&self, key: &EthHash, value: &[u8]) -> anyhow::Result<()> {
        self.inner.write_bin(key, value)
    }

    fn exists(&self, key: &EthHash) -> anyhow::Result<bool> {
        self.inner.exists(key)
    }

    fn get_message_cids(&self) -> anyhow::Result<Vec<(Cid, u64)>> {
        self.inner.get_message_cids()
    }

    fn delete(&self, keys: Vec<EthHash>) -> anyhow::Result<()> {
        self.inner.delete(keys)
    }
}
