use std::fs;
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};

use filecoin_proofs::error::ExpectWithBacktrace;
use filecoin_proofs::types::{PoRepConfig, PoStConfig, SectorClass};
use storage_proofs::sector::SectorId;

use crate::constants::*;
use crate::disk_backed_storage::new_sector_store;
use crate::error::{Result, SectorBuilderErr};
use crate::helpers;
use crate::helpers::SnapshotKey;
use crate::kv_store::{KeyValueStore, SledKvs};
use crate::metadata::*;
use crate::metadata_manager::SectorMetadataManager;
use crate::scheduler::{PerformHealthCheck, Scheduler, SchedulerTask};
use crate::state::SectorBuilderState;
use crate::worker::*;
use crate::SectorStore;
use std::io::Read;

const FATAL_NOLOAD: &str = "could not load snapshot";

pub struct SectorBuilder<T: Read + Send> {
    // Prevents FFI consumers from queueing behind long-running seal operations.
    worker_tx: mpsc::Sender<WorkerTask>,

    // For additional seal concurrency, add more workers here.
    workers: Vec<Worker>,

    // The main worker's queue.
    scheduler_tx: mpsc::SyncSender<SchedulerTask<T>>,

    // The main worker. Owns all mutable state for the SectorBuilder.
    scheduler: Scheduler,
}

impl<R: 'static + Send + std::io::Read> SectorBuilder<R> {
    // Initialize and return a SectorBuilder from metadata persisted to disk if
    // it exists. Otherwise, initialize and return a fresh SectorBuilder. The
    // metadata key is equal to the prover_id.
    #[allow(clippy::too_many_arguments)]
    pub fn init_from_metadata<P: AsRef<Path>>(
        sector_class: SectorClass,
        last_committed_sector_id: SectorId,
        metadata_dir: P,
        prover_id: [u8; 32],
        sealed_sector_dir: P,
        staged_sector_dir: P,
        sector_cache_root: P,
        max_num_staged_sectors: u8,
    ) -> Result<SectorBuilder<R>> {
        ensure_parameter_cache_hydrated(sector_class)?;

        // Configure the scheduler's rendezvous channel.
        let (scheduler_tx, scheduler_rx) = mpsc::sync_channel(0);

        // Configure workers and channels.
        let (worker_tx, workers) = {
            let (tx, rx) = mpsc::channel();
            let rx = Arc::new(Mutex::new(rx));

            let workers = (0..NUM_WORKERS)
                .map(|n| Worker::start(n, rx.clone(), prover_id))
                .collect();

            (tx, workers)
        };

        let sector_size = sector_class.0.into();

        // Initialize the key/value store in which we store metadata
        // snapshots.
        let kv_store =
            SledKvs::initialize(metadata_dir.as_ref()).expect("failed to initialize K/V store");

        // Initialize a SectorStore and wrap it in an Arc so we can access it
        // from multiple threads. Our implementation assumes that the
        // SectorStore is safe for concurrent access.
        let sector_store = new_sector_store(
            sector_class,
            sealed_sector_dir,
            staged_sector_dir,
            sector_cache_root,
        );

        // Build the scheduler's initial state. If available, we
        // reconstitute this state from persisted metadata. If not, we
        // create it from scratch.
        let state = {
            let loaded =
                helpers::load_snapshot(&kv_store, &SnapshotKey::new(prover_id, sector_size))
                    .expects(FATAL_NOLOAD)
                    .map(Into::into);

            loaded.unwrap_or_else(|| SectorBuilderState::new(last_committed_sector_id))
        };

        let max_user_bytes_per_staged_sector =
            sector_store.sector_config().max_unsealed_bytes_per_sector();

        let m = SectorMetadataManager::initialize(
            kv_store,
            sector_store,
            state,
            max_num_staged_sectors,
            max_user_bytes_per_staged_sector,
            prover_id,
            sector_size,
        );

        let scheduler = Scheduler::start(scheduler_tx.clone(), scheduler_rx, worker_tx.clone(), m)?;

        Ok(SectorBuilder {
            scheduler_tx,
            scheduler,
            worker_tx,
            workers,
        })
    }

    /// TODO: document this
    pub fn resume_seal_sector(&self, sector_id: SectorId) -> Result<SealedSectorMetadata> {
        log_unrecov(self.run_blocking(|tx| SchedulerTask::ResumeSealSector(sector_id, tx)))
            .and_then(|x| {
                x.first()
                    .cloned()
                    .ok_or_else(|| format_err!("resume_seal_sector expected one sector"))
            })
    }

    /// TODO: document this
    pub fn seal_sector(
        &self,
        sector_id: SectorId,
        seal_ticket: SealTicket,
    ) -> Result<SealedSectorMetadata> {
        log_unrecov(self.run_blocking(|tx| SchedulerTask::SealSector(sector_id, seal_ticket, tx)))
            .and_then(|x| {
                x.first()
                    .cloned()
                    .ok_or_else(|| format_err!("seal_sector expected one sector"))
            })
    }

    // Stages user piece-bytes for sealing. Note that add_piece calls are
    // processed sequentially to make bin packing easier.
    pub fn add_piece(
        &self,
        piece_key: String,
        piece_file: R,
        piece_bytes_amount: u64,
        store_until: SecondsSinceEpoch,
    ) -> Result<SectorId> {
        log_unrecov(self.run_blocking(|tx| {
            SchedulerTask::AddPiece(piece_key, piece_bytes_amount, piece_file, store_until, tx)
        }))
    }

    // Returns sealing status for the sector with specified id. If no sealed or
    // staged sector exists with the provided id, produce an error.
    pub fn get_seal_status(&self, sector_id: SectorId) -> Result<SealStatus> {
        log_unrecov(self.run_blocking(|tx| SchedulerTask::GetSealStatus(sector_id, tx)))
    }

    // Unseals the sector containing the referenced piece and returns its
    // bytes. Produces an error if this sector builder does not have a sealed
    // sector containing the referenced piece.
    pub fn read_piece_from_sealed_sector(&self, piece_key: String) -> Result<Vec<u8>> {
        log_unrecov(self.run_blocking(|tx| SchedulerTask::RetrievePiece(piece_key, tx)))
    }

    // For demo purposes. Schedules sealing of all staged sectors, blocking
    // until complete.
    pub fn seal_all_staged_sectors(
        &self,
        seal_ticket: SealTicket,
    ) -> Result<Vec<SealedSectorMetadata>> {
        log_unrecov(self.run_blocking(|tx| SchedulerTask::SealAllStagedSectors(seal_ticket, tx)))
    }

    // Returns all sealed sector metadata.
    pub fn get_sealed_sectors(&self, check_health: bool) -> Result<Vec<GetSealedSectorResult>> {
        log_unrecov(self.run_blocking(|tx| {
            SchedulerTask::GetSealedSectors(PerformHealthCheck(check_health), tx)
        }))
    }

    // Returns all staged sector metadata.
    pub fn get_staged_sectors(&self) -> Result<Vec<StagedSectorMetadata>> {
        log_unrecov(self.run_blocking(SchedulerTask::GetStagedSectors))
    }

    // Generates a proof-of-spacetime.
    pub fn generate_post(
        &self,
        comm_rs: &[[u8; 32]],
        challenge_seed: &[u8; 32],
        faults: Vec<SectorId>,
    ) -> Result<Vec<u8>> {
        log_unrecov(self.run_blocking(|tx| {
            SchedulerTask::GeneratePoSt(Vec::from(comm_rs), *challenge_seed, faults, tx)
        }))
    }

    // Run a task, blocking on the return channel.
    fn run_blocking<T, F: FnOnce(mpsc::SyncSender<T>) -> SchedulerTask<R>>(
        &self,
        with_sender: F,
    ) -> T {
        let (tx, rx) = mpsc::sync_channel(0);

        self.scheduler_tx
            .clone()
            .send(with_sender(tx))
            .expects(FATAL_NOSEND_TASK);

        rx.recv().expects(FATAL_NORECV_TASK)
    }
}

impl<T: Read + Send> Drop for SectorBuilder<T> {
    fn drop(&mut self) {
        // Shut down main worker and sealers, too.
        let _ = self
            .scheduler_tx
            .send(SchedulerTask::Shutdown)
            .map_err(|err| println!("err sending Shutdown to scheduler: {:?}", err));

        for _ in &mut self.workers {
            let _ = self
                .worker_tx
                .send(WorkerTask::Shutdown)
                .map_err(|err| println!("err sending Shutdown to sealer: {:?}", err));
        }

        // Wait for worker threads to return.
        let scheduler_thread = &mut self.scheduler.thread;

        if let Some(thread) = scheduler_thread.take() {
            let _ = thread
                .join()
                .map_err(|err| println!("err joining scheduler thread: {:?}", err));
        }

        for worker in &mut self.workers {
            if let Some(thread) = worker.thread.take() {
                let _ = thread
                    .join()
                    .map_err(|err| println!("err joining sealer thread: {:?}", err));
            }
        }
    }
}

/// Checks the parameter cache for the given sector size.
/// Returns an `Err` if it is not hydrated.
fn ensure_parameter_cache_hydrated(sector_class: SectorClass) -> Result<()> {
    // PoRep
    let porep_config: PoRepConfig = sector_class.into();

    let porep_cache_key = porep_config.get_cache_verifying_key_path();
    ensure_file(porep_cache_key)
        .map_err(|err| format_err!("missing verifying key for PoRep: {:?}", err))?;

    let porep_cache_params = porep_config.get_cache_params_path();
    ensure_file(porep_cache_params)
        .map_err(|err| format_err!("missing Groth parameters for PoRep: {:?}", err))?;

    // PoSt
    let post_config: PoStConfig = sector_class.into();

    let post_cache_key = post_config.get_cache_verifying_key_path();
    ensure_file(post_cache_key)
        .map_err(|err| format_err!("missing verifying key for PoSt: {:?}", err))?;

    let post_cache_params = post_config.get_cache_params_path();
    ensure_file(post_cache_params)
        .map_err(|err| format_err!("missing Groth parameters for PoSt: {:?}", err))?;

    Ok(())
}

fn log_unrecov<T>(result: Result<T>) -> Result<T> {
    if let Err(err) = &result {
        if let Some(SectorBuilderErr::Unrecoverable(err, backtrace)) = err.downcast_ref() {
            error!("unrecoverable: {:?} - {:?}", err, backtrace);
        }
    }

    result
}

fn ensure_file(p: impl AsRef<Path>) -> Result<()> {
    let path_str = p.as_ref().to_string_lossy();

    let metadata =
        fs::metadata(p.as_ref()).map_err(|_| format_err!("Failed to stat: {}", path_str))?;

    ensure!(metadata.is_file(), "Not a file: {}", path_str);
    ensure!(metadata.len() > 0, "Empty file: {}", path_str);

    Ok(())
}

#[cfg(test)]
pub mod tests {
    use filecoin_proofs::{PoRepProofPartitions, SectorSize};

    use super::*;

    #[test]
    fn test_cannot_init_sector_builder_without_empty_parameter_cache() {
        let temp_dir = tempfile::tempdir()
            .unwrap()
            .path()
            .to_str()
            .unwrap()
            .to_string();

        let nonsense_sector_class = SectorClass(SectorSize(32), PoRepProofPartitions(123));

        let result = SectorBuilder::<std::fs::File>::init_from_metadata(
            nonsense_sector_class,
            SectorId::from(0),
            temp_dir.clone(),
            [0u8; 32],
            temp_dir.clone(),
            temp_dir.clone(),
            temp_dir,
            1,
        );

        assert!(result.is_err());
    }
}
