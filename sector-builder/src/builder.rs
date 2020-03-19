use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};

use filecoin_proofs::constants::*;
use filecoin_proofs::types::{PoRepConfig, PoStConfig, SectorClass};
use filecoin_proofs::Candidate;
use storage_proofs::sector::SectorId;

use crate::constants::*;
use crate::disk_backed_storage::new_sector_store;
use crate::error::{Result, SectorBuilderErr};
use crate::helpers;
use crate::helpers::SnapshotKey;
use crate::kv_store::{FileSystemKvs, KeyValueStore};
use crate::metadata::*;
use crate::metadata_manager::SectorMetadataManager;
use crate::scheduler::{PerformHealthCheck, Scheduler, SchedulerTask};
use crate::state::SectorBuilderState;
use crate::worker::*;
use std::io::Read;

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
        num_workers: u8,
    ) -> Result<SectorBuilder<R>> {
        let porep_config = sector_class.into();
        let post_config = PoStConfig {
            sector_size: sector_class.sector_size,
            challenge_count: POST_CHALLENGE_COUNT,
            challenged_nodes: POST_CHALLENGED_NODES,
            priority: true,
        };
        ensure_parameter_cache_hydrated(porep_config, post_config)?;

        // Configure the scheduler's rendezvous channel.
        let (scheduler_tx, scheduler_rx) = mpsc::sync_channel(0);

        // Configure workers and channels.
        let (worker_tx, workers) = {
            let (tx, rx) = mpsc::channel();
            let rx = Arc::new(Mutex::new(rx));

            let workers = (0..num_workers)
                .map(|n| Worker::start(n, rx.clone(), prover_id))
                .collect();

            (tx, workers)
        };

        let sector_size = sector_class.sector_size.into();

        // Initialize the key/value store in which we store metadata
        // snapshots.
        let kv_store = FileSystemKvs::initialize(metadata_dir.as_ref())
            .map_err(|err| format_err!("could not initialize metadata store: {:?}", err))?;

        // Initialize a SectorStore and wrap it in an Arc so we can access it
        // from multiple threads. Our implementation assumes that the
        // SectorStore is safe for concurrent access.
        let sector_store = new_sector_store(
            sector_class,
            sealed_sector_dir,
            staged_sector_dir,
            sector_cache_root,
        );

        // Build the scheduler's initial state. If available, we reconstitute
        // this state from persisted metadata. If not, we create it from
        // scratch.
        let loaded: Option<SectorBuilderState> =
            helpers::load_snapshot(&kv_store, &SnapshotKey::new(prover_id, sector_size))
                .map_err(|err| format_err!("failed to load metadata snapshot: {}", err))
                .map(Into::into)?;

        let state = if let Some(inner) = loaded {
            inner
        } else {
            SectorBuilderState::new(last_committed_sector_id)
        };

        let max_user_bytes_per_staged_sector =
            sector_store.sector_config().max_unsealed_bytes_per_sector;

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

    // Sends a pre-commit command to the main runloop and blocks until complete.
    pub fn seal_pre_commit(
        &self,
        sector_id: SectorId,
        ticket: SealTicket,
    ) -> Result<StagedSectorMetadata> {
        log_unrecov(self.run_blocking(|tx| SchedulerTask::SealPreCommit(sector_id, ticket, tx)))
    }

    // Sends a commit command to the main runloop and blocks until complete.
    pub fn seal_commit(&self, sector_id: SectorId, seed: SealSeed) -> Result<SealedSectorMetadata> {
        log_unrecov(self.run_blocking(|tx| SchedulerTask::SealCommit(sector_id, seed, tx)))
    }

    // Sends a pre-commit resumption command to the main runloop and blocks
    // until complete.
    pub fn resume_seal_pre_commit(&self, sector_id: SectorId) -> Result<StagedSectorMetadata> {
        log_unrecov(self.run_blocking(|tx| SchedulerTask::ResumeSealPreCommit(sector_id, tx)))
    }

    // Sends a resume seal command to the main runloop and blocks until
    // complete.
    pub fn resume_seal_commit(&self, sector_id: SectorId) -> Result<SealedSectorMetadata> {
        log_unrecov(self.run_blocking(|tx| SchedulerTask::ResumeSealCommit(sector_id, tx)))
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

    // Generates election candidates.
    pub fn generate_candidates(
        &self,
        comm_rs: &[[u8; 32]],
        challenge_seed: &[u8; 32],
        challenge_count: u64,
        faults: Vec<SectorId>,
    ) -> Result<Vec<Candidate>> {
        log_unrecov(self.run_blocking(|tx| {
            SchedulerTask::GenerateCandidates(
                Vec::from(comm_rs),
                *challenge_seed,
                challenge_count,
                faults,
                tx,
            )
        }))
    }

    // Generates a proof-of-spacetime.
    pub fn generate_post(
        &self,
        comm_rs: &[[u8; 32]],
        challenge_seed: &[u8; 32],
        challenge_count: u64,
        winners: Vec<Candidate>,
    ) -> Result<Vec<Vec<u8>>> {
        log_unrecov(self.run_blocking(|tx| {
            SchedulerTask::GeneratePoSt(
                Vec::from(comm_rs),
                *challenge_seed,
                challenge_count,
                winners,
                tx,
            )
        }))
    }

    // Increments the manager's nonce and returns a newly-provisioned sector id.
    pub fn acquire_sector_id(&self) -> Result<SectorId> {
        log_unrecov(self.run_blocking(SchedulerTask::AcquireSectorId))
    }

    // Imports a sector sealed elsewhere. This function uses the rename system
    // call to take ownership of the cache directory and sealed sector file.
    #[allow(clippy::too_many_arguments)]
    pub fn import_sealed_sector(
        &self,
        sector_id: SectorId,
        sector_cache_dir: PathBuf,
        sealed_sector: PathBuf,
        seal_ticket: SealTicket,
        seal_seed: SealSeed,
        comm_r: [u8; 32],
        comm_d: [u8; 32],
        pieces: Vec<PieceMetadata>,
        proof: Vec<u8>,
    ) -> Result<()> {
        log_unrecov(self.run_blocking(|tx| SchedulerTask::ImportSector {
            sector_id,
            sector_cache_dir,
            sealed_sector,
            seal_ticket,
            seal_seed,
            comm_r,
            comm_d,
            pieces,
            proof,
            done_tx: tx,
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
            .expect(FATAL_NOSEND_TASK);

        rx.recv().expect(FATAL_NORECV_TASK)
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
fn ensure_parameter_cache_hydrated(
    porep_config: PoRepConfig,
    post_config: PoStConfig,
) -> Result<()> {
    // PoRep
    let porep_cache_key = porep_config.get_cache_verifying_key_path()?;
    ensure_file(porep_cache_key)
        .map_err(|err| format_err!("missing verifying key for PoRep: {:?}", err))?;

    let porep_cache_params = porep_config.get_cache_params_path()?;
    ensure_file(porep_cache_params)
        .map_err(|err| format_err!("missing Groth parameters for PoRep: {:?}", err))?;

    // PoSt
    let post_cache_key = post_config.get_cache_verifying_key_path()?;
    ensure_file(post_cache_key)
        .map_err(|err| format_err!("missing verifying key for PoSt: {:?}", err))?;

    let post_cache_params = post_config.get_cache_params_path()?;
    ensure_file(post_cache_params)
        .map_err(|err| format_err!("missing Groth parameters for PoSt: {:?}", err))?;

    Ok(())
}

fn log_unrecov<T>(result: Result<T>) -> Result<T> {
    if let Err(err) = &result {
        if let Some(SectorBuilderErr::Unrecoverable(err)) = err.downcast_ref() {
            error!("unrecoverable: {:?} - {:?}", err, err.backtrace());
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
    use std::io::Write;

    #[ignore]
    #[test]
    fn test_cannot_init_sector_builder_with_corrupted_snapshot() {
        let f = || {
            tempfile::tempdir()
                .unwrap()
                .into_path()
                .to_str()
                .unwrap()
                .to_string()
        };

        let meta_dir = f();
        let sealed_dir = f();
        let staged_dir = f();
        let cache_root_dir = f();

        let sector_builder = SectorBuilder::init_from_metadata(
            SectorClass {
                sector_size: SectorSize(2048),
                partitions: PoRepProofPartitions(2),
            },
            SectorId::from(0),
            &meta_dir,
            [0u8; 32],
            &sealed_dir,
            &staged_dir,
            &cache_root_dir,
            1,
            2,
        )
        .expect("cannot create sector builder");

        sector_builder
            .add_piece(
                "foo".into(),
                std::io::repeat(42).take(1016),
                1016,
                SecondsSinceEpoch(0),
            )
            .expect("piece add failed");

        // destroy the first builder instance
        std::mem::drop(sector_builder);

        // corrupt the snapshot file
        for path in std::fs::read_dir(&meta_dir).unwrap() {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .read(true)
                .open(path.unwrap().path().display().to_string())
                .expect("could not open");

            f.write_all(b"eat at joe's").expect("could not write");
        }

        // instantiate a second builder
        let init_result = SectorBuilder::<std::fs::File>::init_from_metadata(
            SectorClass {
                sector_size: SectorSize(1024),
                partitions: PoRepProofPartitions(2),
            },
            SectorId::from(0),
            &meta_dir,
            [0u8; 32],
            &sealed_dir,
            &staged_dir,
            &cache_root_dir,
            1,
            2,
        );

        assert!(
            init_result.is_err(),
            "corrupted snapshot must cause an error"
        );
    }

    #[test]
    fn test_cannot_init_sector_builder_with_empty_parameter_cache() {
        let temp_dir = tempfile::tempdir()
            .unwrap()
            .path()
            .to_str()
            .unwrap()
            .to_string();

        let nonsense_sector_class = SectorClass {
            sector_size: SectorSize(2048),
            partitions: PoRepProofPartitions(123),
        };

        let result = SectorBuilder::<std::fs::File>::init_from_metadata(
            nonsense_sector_class,
            SectorId::from(0),
            temp_dir.clone(),
            [0u8; 32],
            temp_dir.clone(),
            temp_dir.clone(),
            temp_dir,
            1,
            2,
        );

        assert!(result.is_err());
    }
}
