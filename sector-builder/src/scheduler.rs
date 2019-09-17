use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::thread;

use filecoin_proofs::error::ExpectWithBacktrace;
use filecoin_proofs::{generate_post, PrivateReplicaInfo};
use storage_proofs::sector::SectorId;

use crate::builder::WrappedKeyValueStore;
use crate::error::{err_piecenotfound, err_unrecov, Result};
use crate::helpers::{
    add_piece, get_seal_status, get_sealed_sector_health, get_sectors_ready_for_sealing,
    load_snapshot, persist_snapshot, SnapshotKey,
};
use crate::kv_store::KeyValueStore;
use crate::metadata::{SealStatus, SealedSectorMetadata, StagedSectorMetadata};
use crate::sealer::SealerInput;
use crate::state::{SectorBuilderState, StagedState};
use crate::store::SectorStore;
use crate::GetSealedSectorResult::WithHealth;
use crate::{GetSealedSectorResult, PaddedBytesAmount, SecondsSinceEpoch, UnpaddedBytesAmount};
use filecoin_proofs::pieces::get_piece_start_byte;

const FATAL_NOLOAD: &str = "could not load snapshot";
const FATAL_NORECV: &str = "could not receive task";
const FATAL_NOSEND: &str = "could not send";
const FATAL_SNPSHT: &str = "could not snapshot";
const FATAL_SLRSND: &str = "could not send to sealer";
const FATAL_HUNGUP: &str = "could not send to ret channel";
const FATAL_NOSECT: &str = "could not find sector";

pub struct Scheduler {
    pub thread: Option<thread::JoinHandle<()>>,
}

#[derive(Debug)]
pub struct PerformHealthCheck(pub bool);

#[derive(Debug)]
pub enum Request {
    AddPiece(
        String,
        u64,
        String,
        SecondsSinceEpoch,
        mpsc::SyncSender<Result<SectorId>>,
    ),
    GetSealedSectors(
        PerformHealthCheck,
        mpsc::SyncSender<Result<Vec<GetSealedSectorResult>>>,
    ),
    GetStagedSectors(mpsc::SyncSender<Result<Vec<StagedSectorMetadata>>>),
    GetSealStatus(SectorId, mpsc::SyncSender<Result<SealStatus>>),
    GeneratePoSt(
        Vec<[u8; 32]>,
        [u8; 32],      // seed
        Vec<SectorId>, // faults
        mpsc::SyncSender<Result<Vec<u8>>>,
    ),
    RetrievePiece(String, mpsc::SyncSender<Result<Vec<u8>>>),
    SealAllStagedSectors(mpsc::SyncSender<Result<()>>),
    HandleSealResult(SectorId, Box<Result<SealedSectorMetadata>>),
    HandleRetrievePieceResult(
        Result<(UnpaddedBytesAmount, PathBuf)>,
        mpsc::SyncSender<Result<Vec<u8>>>,
    ),
    Shutdown,
}

impl Scheduler {
    #[allow(clippy::too_many_arguments)]
    pub fn start_with_metadata<T: 'static + KeyValueStore, S: 'static + SectorStore>(
        scheduler_input_rx: mpsc::Receiver<Request>,
        scheduler_input_tx: mpsc::SyncSender<Request>,
        sealer_input_tx: mpsc::Sender<SealerInput>,
        kv_store: Arc<WrappedKeyValueStore<T>>,
        sector_store: Arc<S>,
        last_committed_sector_id: SectorId,
        max_num_staged_sectors: u8,
        prover_id: [u8; 31],
        sector_size: PaddedBytesAmount,
    ) -> Scheduler {
        let thread = thread::spawn(move || {
            // Build the scheduler's initial state. If available, we
            // reconstitute this state from persisted metadata. If not, we
            // create it from scratch.
            let state = {
                let loaded = load_snapshot(&kv_store, &SnapshotKey::new(prover_id, sector_size))
                    .expects(FATAL_NOLOAD)
                    .map(Into::into);

                loaded.unwrap_or_else(|| SectorBuilderState {
                    staged: StagedState {
                        sector_id_nonce: u64::from(last_committed_sector_id),
                        sectors: Default::default(),
                    },
                    sealed: Default::default(),
                })
            };

            let max_user_bytes_per_staged_sector =
                sector_store.sector_config().max_unsealed_bytes_per_sector();

            let mut m = SectorMetadataManager {
                kv_store,
                sector_store,
                state,
                sealer_input_tx,
                scheduler_input_tx: scheduler_input_tx.clone(),
                max_num_staged_sectors,
                max_user_bytes_per_staged_sector,
                prover_id,
                sector_size,
            };

            loop {
                let task = scheduler_input_rx.recv().expects(FATAL_NORECV);

                // Dispatch to the appropriate task-handler.
                match task {
                    Request::AddPiece(key, amt, path, store_until, tx) => {
                        tx.send(m.add_piece(key, amt, path, store_until))
                            .expects(FATAL_NOSEND);
                    }
                    Request::GetSealStatus(sector_id, tx) => {
                        tx.send(m.get_seal_status(sector_id)).expects(FATAL_NOSEND);
                    }
                    Request::RetrievePiece(piece_key, tx) => m.retrieve_piece(piece_key, tx),
                    Request::GetSealedSectors(check_health, tx) => {
                        tx.send(m.get_sealed_sectors(check_health.0))
                            .expects(FATAL_NOSEND);
                    }
                    Request::GetStagedSectors(tx) => {
                        tx.send(m.get_staged_sectors()).expect(FATAL_NOSEND);
                    }
                    Request::SealAllStagedSectors(tx) => {
                        tx.send(m.seal_all_staged_sectors()).expects(FATAL_NOSEND);
                    }
                    Request::HandleSealResult(sector_id, result) => {
                        m.handle_seal_result(sector_id, *result);
                    }
                    Request::HandleRetrievePieceResult(result, tx) => {
                        m.handle_retrieve_piece_result(result, tx);
                    }
                    Request::GeneratePoSt(comm_rs, chg_seed, faults, tx) => {
                        m.generate_post(&comm_rs, &chg_seed, faults, tx)
                    }
                    Request::Shutdown => break,
                }
            }
        });

        Scheduler {
            thread: Some(thread),
        }
    }
}

// The SectorBuilderStateManager is the owner of all sector-related metadata.
// It dispatches expensive operations (e.g. unseal and seal) to the sealer
// worker-threads. Other, inexpensive work (or work which needs to be performed
// serially) is handled by the SectorBuilderStateManager itself.
pub struct SectorMetadataManager<T: KeyValueStore, S: SectorStore> {
    kv_store: Arc<WrappedKeyValueStore<T>>,
    sector_store: Arc<S>,
    state: SectorBuilderState,
    sealer_input_tx: mpsc::Sender<SealerInput>,
    scheduler_input_tx: mpsc::SyncSender<Request>,
    max_num_staged_sectors: u8,
    max_user_bytes_per_staged_sector: UnpaddedBytesAmount,
    prover_id: [u8; 31],
    sector_size: PaddedBytesAmount,
}

impl<T: KeyValueStore, S: SectorStore> SectorMetadataManager<T, S> {
    pub fn generate_post(
        &self,
        comm_rs: &[[u8; 32]],
        challenge_seed: &[u8; 32],
        faults: Vec<SectorId>,
        return_channel: mpsc::SyncSender<Result<Vec<u8>>>,
    ) {
        let fault_set: HashSet<SectorId> = faults.into_iter().collect();

        let comm_rs_set: HashSet<&[u8; 32]> = comm_rs.iter().collect();

        let mut replicas: BTreeMap<SectorId, PrivateReplicaInfo> = Default::default();

        for sector in self.state.sealed.sectors.values() {
            if comm_rs_set.contains(&sector.comm_r) {
                let path_str = self
                    .sector_store
                    .manager()
                    .sealed_sector_path(&sector.sector_access)
                    .to_str()
                    .map(str::to_string)
                    .unwrap();

                let info = if fault_set.contains(&sector.sector_id) {
                    PrivateReplicaInfo::new_faulty(path_str, sector.comm_r)
                } else {
                    PrivateReplicaInfo::new(path_str, sector.comm_r)
                };

                replicas.insert(sector.sector_id, info);
            }
        }

        let output = generate_post(
            self.sector_store.proofs_config().post_config(),
            challenge_seed,
            &replicas,
        );

        // TODO: Where should this work be scheduled? New worker type?
        return_channel.send(output).expects(FATAL_HUNGUP);
    }

    // Schedules unseal on worker thread. Any errors encountered while
    // retrieving are send to done_tx.
    pub fn retrieve_piece(&self, piece_key: String, done_tx: mpsc::SyncSender<Result<Vec<u8>>>) {
        let done_tx_c = done_tx.clone();

        let task = Ok(()).and_then(|_| {
            let opt_sealed_sector = self.state.sealed.sectors.values().find(|sector| {
                sector
                    .pieces
                    .iter()
                    .any(|piece| piece.piece_key == piece_key)
            });

            opt_sealed_sector
                .ok_or_else(|| err_piecenotfound(piece_key.to_string()).into())
                .and_then(|sealed_sector| {
                    let piece = sealed_sector
                        .pieces
                        .iter()
                        .find(|p| p.piece_key == piece_key)
                        .ok_or_else(|| err_piecenotfound(piece_key.clone()))?;

                    let piece_lengths: Vec<_> = sealed_sector
                        .pieces
                        .iter()
                        .take_while(|p| p.piece_key != piece_key)
                        .map(|p| p.num_bytes)
                        .collect();

                    let staged_sector_access = self
                        .sector_store
                        .manager()
                        .new_staging_sector_access(sealed_sector.sector_id)
                        .map_err(failure::Error::from)?;

                    Ok(SealerInput::Unseal {
                        porep_config: self.sector_store.proofs_config().porep_config(),
                        source_path: self
                            .sector_store
                            .manager()
                            .sealed_sector_path(&sealed_sector.sector_access),
                        destination_path: self
                            .sector_store
                            .manager()
                            .staged_sector_path(&staged_sector_access),
                        sector_id: sealed_sector.sector_id,
                        piece_start_byte: get_piece_start_byte(&piece_lengths, piece.num_bytes),
                        piece_len: piece.num_bytes,
                        caller_done_tx: done_tx_c,
                        done_tx: self.scheduler_input_tx.clone(),
                    })
                })
        });

        match task {
            Ok(task) => self
                .sealer_input_tx
                .clone()
                .send(task)
                .expects(FATAL_SLRSND),
            Err(err) => {
                self.scheduler_input_tx
                    .send(Request::HandleRetrievePieceResult(Err(err), done_tx))
                    .expects(FATAL_SLRSND);
            }
        }
    }

    // Returns sealing status for the sector with specified id. If no sealed or
    // staged sector exists with the provided id, produce an error.
    pub fn get_seal_status(&self, sector_id: SectorId) -> Result<SealStatus> {
        get_seal_status(&self.state.staged, &self.state.sealed, sector_id)
    }

    // Write the piece to storage, obtaining the sector id with which the
    // piece-bytes are now associated.
    pub fn add_piece(
        &mut self,
        piece_key: String,
        piece_bytes_amount: u64,
        piece_path: String,
        store_until: SecondsSinceEpoch,
    ) -> Result<SectorId> {
        let destination_sector_id = add_piece(
            &self.sector_store,
            &mut self.state.staged,
            piece_key,
            piece_bytes_amount,
            piece_path,
            store_until,
        )?;

        self.check_and_schedule(false)?;
        self.checkpoint()?;

        Ok(destination_sector_id)
    }

    // For demo purposes. Schedules sealing of all staged sectors.
    pub fn seal_all_staged_sectors(&mut self) -> Result<()> {
        self.check_and_schedule(true)?;
        self.checkpoint()
    }

    // Produces a vector containing metadata for all sealed sectors that this
    // SectorBuilder knows about. Includes sector health-information on request.
    pub fn get_sealed_sectors(&self, check_health: bool) -> Result<Vec<GetSealedSectorResult>> {
        use rayon::prelude::*;

        let sectors_iter = self.state.sealed.sectors.values().cloned();

        if !check_health {
            return Ok(sectors_iter
                .map(GetSealedSectorResult::WithoutHealth)
                .collect());
        }

        let with_path: Vec<(PathBuf, SealedSectorMetadata)> = sectors_iter
            .map(|meta| {
                let pbuf = self
                    .sector_store
                    .manager()
                    .sealed_sector_path(&meta.sector_access);

                (pbuf, meta)
            })
            .collect();

        // compute sector health in parallel using workers from rayon global
        // thread pool
        with_path
            .into_par_iter()
            .map(|(pbuf, meta)| {
                let health = get_sealed_sector_health(&pbuf, &meta)?;
                Ok(WithHealth(health, meta))
            })
            .collect()
    }

    // Produces a vector containing metadata for all staged sectors that this
    // SectorBuilder knows about.
    pub fn get_staged_sectors(&self) -> Result<Vec<StagedSectorMetadata>> {
        Ok(self.state.staged.sectors.values().cloned().collect())
    }

    // Update metadata to reflect the sealing results.
    pub fn handle_retrieve_piece_result(
        &mut self,
        result: Result<(UnpaddedBytesAmount, PathBuf)>,
        return_channel: mpsc::SyncSender<Result<Vec<u8>>>,
    ) {
        let result = result.and_then(|(n, pbuf)| {
            let buffer = self.sector_store.manager().read_raw(
                pbuf.to_str()
                    .ok_or_else(|| format_err!("conversion failed"))?,
                0,
                n,
            )?;

            Ok(buffer)
        });

        return_channel.send(result).expects(FATAL_NOSEND);
    }

    // Update metadata to reflect the sealing results.
    pub fn handle_seal_result(
        &mut self,
        sector_id: SectorId,
        result: Result<SealedSectorMetadata>,
    ) {
        // scope exists to end the mutable borrow of self so that we can
        // checkpoint
        {
            let staged_state = &mut self.state.staged;
            let sealed_state = &mut self.state.sealed;

            match result {
                Err(err) => {
                    if let Some(staged_sector) = staged_state.sectors.get_mut(&sector_id) {
                        staged_sector.seal_status =
                            SealStatus::Failed(format!("{}", err_unrecov(err)));
                    };
                }
                Ok(sealed_sector) => {
                    sealed_state.sectors.insert(sector_id, sealed_sector);
                }
            };
        }

        self.checkpoint().expects(FATAL_SNPSHT);
    }

    // Check for sectors which should no longer receive new user piece-bytes and
    // schedule them for sealing.
    fn check_and_schedule(&mut self, seal_all_staged_sectors: bool) -> Result<()> {
        let staged_state = &mut self.state.staged;

        let to_be_sealed = get_sectors_ready_for_sealing(
            staged_state,
            self.max_user_bytes_per_staged_sector,
            self.max_num_staged_sectors,
            seal_all_staged_sectors,
        );

        // Mark the to-be-sealed sectors as no longer accepting data and then
        // schedule sealing.
        for sector_id in to_be_sealed {
            let mut sector = staged_state
                .sectors
                .get_mut(&sector_id)
                .expects(FATAL_NOSECT);
            sector.seal_status = SealStatus::Sealing;

            self.sealer_input_tx
                .clone()
                .send(SealerInput::Seal(
                    sector.clone(),
                    self.scheduler_input_tx.clone(),
                ))
                .expects(FATAL_SLRSND);
        }

        Ok(())
    }

    // Create and persist metadata snapshot.
    fn checkpoint(&self) -> Result<()> {
        persist_snapshot(
            &self.kv_store,
            &SnapshotKey::new(self.prover_id, self.sector_size),
            &self.state,
        )?;

        Ok(())
    }
}
