use std::collections::HashMap;
use std::sync::{mpsc, Arc};
use std::thread;

use filecoin_proofs::error::ExpectWithBacktrace;
use filecoin_proofs::generate_post;
use filecoin_proofs::post_adapter::*;

use crate::builder::{SectorId, WrappedKeyValueStore};
use crate::error::{err_piecenotfound, err_unrecov, Result};
use crate::helpers::{
    add_piece, get_seal_status, get_sectors_ready_for_sealing, load_snapshot, persist_snapshot,
    SnapshotKey,
};
use crate::kv_store::KeyValueStore;
use crate::metadata::{SealStatus, SealedSectorMetadata, StagedSectorMetadata};
use crate::sealer::SealerInput;
use crate::state::{SectorBuilderState, StagedState};
use crate::store::SectorStore;
use crate::{PaddedBytesAmount, SecondsSinceEpoch, UnpaddedBytesAmount};

const FATAL_NOLOAD: &str = "could not load snapshot";
const FATAL_NORECV: &str = "could not receive task";
const FATAL_NOSEND: &str = "could not send";
const FATAL_SECMAP: &str = "insert failed";
const FATAL_SNPSHT: &str = "could not snapshot";
const FATAL_SLRSND: &str = "could not send to sealer";
const FATAL_HUNGUP: &str = "could not send to ret channel";
const FATAL_NOSECT: &str = "could not find sector";

pub struct Scheduler {
    pub thread: Option<thread::JoinHandle<()>>,
}

#[derive(Debug)]
pub enum Request {
    AddPiece(
        String,
        u64,
        String,
        SecondsSinceEpoch,
        mpsc::SyncSender<Result<SectorId>>,
    ),
    GetSealedSectors(mpsc::SyncSender<Result<Vec<SealedSectorMetadata>>>),
    GetStagedSectors(mpsc::SyncSender<Result<Vec<StagedSectorMetadata>>>),
    GetSealStatus(SectorId, mpsc::SyncSender<Result<SealStatus>>),
    GeneratePoSt(
        Vec<[u8; 32]>,
        [u8; 32],
        mpsc::SyncSender<Result<GeneratePoStDynamicSectorsCountOutput>>,
    ),
    RetrievePiece(String, mpsc::SyncSender<Result<Vec<u8>>>),
    SealAllStagedSectors(mpsc::SyncSender<Result<()>>),
    HandleSealResult(SectorId, Box<Result<SealedSectorMetadata>>),
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
                        sector_id_nonce: last_committed_sector_id,
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
                    Request::GetSealedSectors(tx) => {
                        tx.send(m.get_sealed_sectors()).expects(FATAL_NOSEND);
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
                    Request::GeneratePoSt(comm_rs, chg_seed, tx) => {
                        m.generate_post(&comm_rs, &chg_seed, tx)
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
        return_channel: mpsc::SyncSender<Result<GeneratePoStDynamicSectorsCountOutput>>,
    ) {
        // reduce our sealed sector state-map to a mapping of comm_r to sealed
        // sector access
        let comm_r_to_sector_access: HashMap<[u8; 32], String> = self
            .state
            .sealed
            .sectors
            .values()
            .fold(HashMap::new(), |mut acc, item| {
                let v = item.sector_access.clone();
                let k = item.comm_r;
                acc.entry(k).or_insert(v);
                acc
            });

        let mut input_parts: Vec<(Option<String>, [u8; 32])> = Default::default();

        for comm_r in comm_rs {
            let access = comm_r_to_sector_access.get(comm_r).and_then(|access| {
                self.sector_store
                    .manager()
                    .sealed_sector_path(access)
                    .to_str()
                    .map(str::to_string)
            });
            input_parts.push((access, *comm_r));
        }

        let mut seed = [0; 32];
        seed.copy_from_slice(challenge_seed);

        let output = generate_post(
            self.sector_store.proofs_config().post_config(),
            seed,
            input_parts,
        );

        // TODO: Where should this work be scheduled? New worker type?
        return_channel.send(output).expects(FATAL_HUNGUP);
    }

    // Unseals the sector containing the referenced piece and returns its
    // bytes. Produces an error if this sector builder does not have a sealed
    // sector containing the referenced piece.
    pub fn retrieve_piece(
        &self,
        piece_key: String,
        return_channel: mpsc::SyncSender<Result<Vec<u8>>>,
    ) {
        let opt_sealed_sector = self.state.sealed.sectors.values().find(|sector| {
            sector
                .pieces
                .iter()
                .any(|piece| piece.piece_key == piece_key)
        });

        if let Some(sealed_sector) = opt_sealed_sector {
            let sealed_sector = Box::new(sealed_sector.clone());
            let task = SealerInput::Unseal(piece_key, sealed_sector, return_channel);

            self.sealer_input_tx
                .clone()
                .send(task)
                .expects(FATAL_SLRSND);
        } else {
            return_channel
                .send(Err(err_piecenotfound(piece_key.to_string()).into()))
                .expects(FATAL_HUNGUP);
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
    ) -> Result<u64> {
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
    // SectorBuilder knows about.
    pub fn get_sealed_sectors(&self) -> Result<Vec<SealedSectorMetadata>> {
        Ok(self.state.sealed.sectors.values().cloned().collect())
    }

    // Produces a vector containing metadata for all staged sectors that this
    // SectorBuilder knows about.
    pub fn get_staged_sectors(&self) -> Result<Vec<StagedSectorMetadata>> {
        Ok(self.state.staged.sectors.values().cloned().collect())
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

            if result.is_err() {
                if let Some(staged_sector) = staged_state.sectors.get_mut(&sector_id) {
                    staged_sector.seal_status =
                        SealStatus::Failed(format!("{}", err_unrecov(result.unwrap_err())));
                };
            } else {
                // Remove the staged sector from the state map.
                let _ = staged_state.sectors.remove(&sector_id);

                // Insert the newly-sealed sector into the other state map.
                let sealed_sector = result.expects(FATAL_SECMAP);

                sealed_state.sectors.insert(sector_id, sealed_sector);
            }
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
