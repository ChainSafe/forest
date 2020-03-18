use std::path::{Path, PathBuf};

use async_std::fs;
use async_std::sync::{channel, Sender};

use anyhow::Context;
use filecoin_proofs::constants::*;
use filecoin_proofs::types::{PoRepConfig, PoStConfig, SectorClass};
use filecoin_proofs::Candidate;
use storage_proofs::sector::SectorId;

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

pub struct SectorBuilder {
    // Prevents FFI consumers from queueing behind long-running seal operations.
    worker_tx: Sender<WorkerTask>,

    // For additional seal concurrency, add more workers here.
    workers: Vec<Worker>,

    // The main worker's queue.
    scheduler_tx: Sender<SchedulerTask>,

    // The main worker. Owns all mutable state for the SectorBuilder.
    scheduler: Scheduler,
}

impl SectorBuilder {
    // Initialize and return a SectorBuilder from metadata persisted to disk if
    // it exists. Otherwise, initialize and return a fresh SectorBuilder. The
    // metadata key is equal to the prover_id.
    #[allow(clippy::too_many_arguments)]
    pub async fn init_from_metadata<P: AsRef<Path>>(
        sector_class: SectorClass,
        last_committed_sector_id: SectorId,
        metadata_dir: P,
        prover_id: [u8; 32],
        sealed_sector_dir: P,
        staged_sector_dir: P,
        sector_cache_root: P,
        max_num_staged_sectors: u8,
        num_workers: u8,
    ) -> Result<SectorBuilder> {
        let porep_config = sector_class.into();
        let post_config = PoStConfig {
            sector_size: sector_class.sector_size,
            challenge_count: POST_CHALLENGE_COUNT,
            challenged_nodes: POST_CHALLENGED_NODES,
            priority: true,
        };
        ensure_parameter_cache_hydrated(porep_config, post_config).await?;

        // Configure the scheduler's rendezvous channel.
        let (scheduler_tx, scheduler_rx) = channel(1);

        // Configure workers and channels.
        let (worker_tx, workers) = {
            let (tx, rx) = channel(100);

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

        let scheduler =
            Scheduler::start(scheduler_tx.clone(), scheduler_rx, worker_tx.clone(), m).await?;

        Ok(SectorBuilder {
            scheduler_tx,
            scheduler,
            worker_tx,
            workers,
        })
    }

    // Sends a pre-commit command to the main runloop and blocks until complete.
    pub async fn seal_pre_commit(
        &self,
        sector_id: SectorId,
        ticket: SealTicket,
    ) -> Result<StagedSectorMetadata> {
        log_unrecov(
            self.run_blocking(|tx| SchedulerTask::SealPreCommit(sector_id, ticket, tx))
                .await,
        )
    }

    // Sends a commit command to the main runloop and blocks until complete.
    pub async fn seal_commit(
        &self,
        sector_id: SectorId,
        seed: SealSeed,
    ) -> Result<SealedSectorMetadata> {
        log_unrecov(
            self.run_blocking(|tx| SchedulerTask::SealCommit(sector_id, seed, tx))
                .await,
        )
    }

    // Sends a pre-commit resumption command to the main runloop and blocks
    // until complete.
    pub async fn resume_seal_pre_commit(
        &self,
        sector_id: SectorId,
    ) -> Result<StagedSectorMetadata> {
        log_unrecov(
            self.run_blocking(|tx| SchedulerTask::ResumeSealPreCommit(sector_id, tx))
                .await,
        )
    }

    // Sends a resume seal command to the main runloop and blocks until
    // complete.
    pub async fn resume_seal_commit(&self, sector_id: SectorId) -> Result<SealedSectorMetadata> {
        log_unrecov(
            self.run_blocking(|tx| SchedulerTask::ResumeSealCommit(sector_id, tx))
                .await,
        )
    }

    // Stages user piece-bytes for sealing. Note that add_piece calls are
    // processed sequentially to make bin packing easier.
    pub async fn add_piece<R: AsRef<Path>>(
        &self,
        piece_key: String,
        piece_path: R,
        piece_bytes_amount: u64,
        store_until: SecondsSinceEpoch,
    ) -> Result<SectorId> {
        log_unrecov(
            self.run_blocking(|tx| {
                SchedulerTask::AddPiece(
                    piece_key,
                    piece_bytes_amount,
                    piece_path.as_ref().to_path_buf(),
                    store_until,
                    tx,
                )
            })
            .await,
        )
    }

    // Returns sealing status for the sector with specified id. If no sealed or
    // staged sector exists with the provided id, produce an error.
    pub async fn get_seal_status(&self, sector_id: SectorId) -> Result<SealStatus> {
        log_unrecov(
            self.run_blocking(|tx| SchedulerTask::GetSealStatus(sector_id, tx))
                .await,
        )
    }

    // Unseals the sector containing the referenced piece and returns its
    // bytes. Produces an error if this sector builder does not have a sealed
    // sector containing the referenced piece.
    pub async fn read_piece_from_sealed_sector(&self, piece_key: String) -> Result<Vec<u8>> {
        log_unrecov(
            self.run_blocking(|tx| SchedulerTask::RetrievePiece(piece_key, tx))
                .await,
        )
    }

    // Returns all sealed sector metadata.
    pub async fn get_sealed_sectors(
        &self,
        check_health: bool,
    ) -> Result<Vec<GetSealedSectorResult>> {
        log_unrecov(
            self.run_blocking(|tx| {
                SchedulerTask::GetSealedSectors(PerformHealthCheck(check_health), tx)
            })
            .await,
        )
    }

    // Returns all staged sector metadata.
    pub async fn get_staged_sectors(&self) -> Result<Vec<StagedSectorMetadata>> {
        log_unrecov(self.run_blocking(SchedulerTask::GetStagedSectors).await)
    }

    // Generates election candidates.
    pub async fn generate_candidates(
        &self,
        comm_rs: &[[u8; 32]],
        challenge_seed: &[u8; 32],
        challenge_count: u64,
        faults: Vec<SectorId>,
    ) -> Result<Vec<Candidate>> {
        log_unrecov(
            self.run_blocking(|tx| {
                SchedulerTask::GenerateCandidates(
                    Vec::from(comm_rs),
                    *challenge_seed,
                    challenge_count,
                    faults,
                    tx,
                )
            })
            .await,
        )
    }

    // Generates a proof-of-spacetime.
    pub async fn generate_post(
        &self,
        comm_rs: &[[u8; 32]],
        challenge_seed: &[u8; 32],
        challenge_count: u64,
        winners: Vec<Candidate>,
    ) -> Result<Vec<Vec<u8>>> {
        log_unrecov(
            self.run_blocking(|tx| {
                SchedulerTask::GeneratePoSt(
                    Vec::from(comm_rs),
                    *challenge_seed,
                    challenge_count,
                    winners,
                    tx,
                )
            })
            .await,
        )
    }

    // Increments the manager's nonce and returns a newly-provisioned sector id.
    pub async fn acquire_sector_id(&self) -> Result<SectorId> {
        log_unrecov(self.run_blocking(SchedulerTask::AcquireSectorId).await)
    }

    // Imports a sector sealed elsewhere. This function uses the rename system
    // call to take ownership of the cache directory and sealed sector file.
    #[allow(clippy::too_many_arguments)]
    pub async fn import_sealed_sector(
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
        log_unrecov(
            self.run_blocking(|tx| SchedulerTask::ImportSector {
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
            })
            .await,
        )
    }

    // Run a task, blocking on the return channel.
    async fn run_blocking<T, F: FnOnce(Sender<T>) -> SchedulerTask>(&self, with_sender: F) -> T {
        let (tx, rx) = channel(1);

        self.scheduler_tx.clone().send(with_sender(tx)).await;

        rx.recv().await.expect("failed to retrieve result")
    }
}

impl Drop for SectorBuilder {
    fn drop(&mut self) {
        async_std::task::block_on(async move {
            // Shut down main worker and sealers, too.
            self.scheduler_tx.send(SchedulerTask::Shutdown).await;

            for _ in &mut self.workers {
                self.worker_tx.send(WorkerTask::Shutdown).await;
            }

            // Wait for worker threads to return.
            let scheduler_thread = &mut self.scheduler.thread;

            if let Some(thread) = scheduler_thread.take() {
                thread.await;
            }

            for worker in &mut self.workers {
                if let Some(thread) = worker.thread.take() {
                    thread.await;
                }
            }
        });
    }
}

/// Checks the parameter cache for the given sector size.
/// Returns an `Err` if it is not hydrated.
async fn ensure_parameter_cache_hydrated(
    porep_config: PoRepConfig,
    post_config: PoStConfig,
) -> Result<()> {
    // PoRep
    let porep_cache_key = porep_config.get_cache_verifying_key_path()?;
    ensure_file(porep_cache_key)
        .await
        .context("missing verifying key for PoRep")?;

    let porep_cache_params = porep_config.get_cache_params_path()?;
    ensure_file(porep_cache_params)
        .await
        .context("missing Groth parameters for PoRep")?;

    // PoSt
    let post_cache_key = post_config.get_cache_verifying_key_path()?;
    ensure_file(post_cache_key)
        .await
        .context("missing verifying key for PoSt")?;

    let post_cache_params = post_config.get_cache_params_path()?;
    ensure_file(post_cache_params)
        .await
        .context("missing Groth parameters for PoSt")?;

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

async fn ensure_file(p: impl AsRef<Path>) -> Result<()> {
    let path_str = p.as_ref().to_string_lossy();

    let metadata = fs::metadata(p.as_ref())
        .await
        .with_context(|| format!("Failed to stat: {}", path_str))?;

    ensure!(metadata.is_file(), "Not a file: {}", path_str);
    ensure!(metadata.len() > 0, "Empty file: {}", path_str);

    Ok(())
}

#[cfg(test)]
pub mod tests {
    use filecoin_proofs::{PoRepProofPartitions, SectorSize};

    use super::*;
    use async_std::prelude::*;

    #[async_std::test]
    #[ignore]
    async fn test_cannot_init_sector_builder_with_corrupted_snapshot() {
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
        .await
        .expect("cannot create sector builder");

        use std::io::Read;

        let mut piece_file = tempfile::NamedTempFile::new().unwrap();
        std::io::copy(
            &mut std::io::repeat(42).take(1016),
            piece_file.as_file_mut(),
        )
        .unwrap();

        sector_builder
            .add_piece("foo".into(), piece_file.path(), 1016, SecondsSinceEpoch(0))
            .await
            .expect("piece add failed");

        // destroy the first builder instance
        std::mem::drop(sector_builder);

        // corrupt the snapshot file
        let mut dirs = fs::read_dir(&meta_dir).await.unwrap();

        while let Some(path) = dirs.next().await {
            let mut f = fs::OpenOptions::new()
                .write(true)
                .read(true)
                .open(path.unwrap().path().display().to_string())
                .await
                .expect("could not open");

            f.write_all(b"eat at joe's").await.expect("could not write");
        }

        // instantiate a second builder
        let init_result = SectorBuilder::init_from_metadata(
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
        )
        .await;

        assert!(
            init_result.is_err(),
            "corrupted snapshot must cause an error"
        );
    }

    #[async_std::test]
    async fn test_cannot_init_sector_builder_with_empty_parameter_cache() {
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

        let result = SectorBuilder::init_from_metadata(
            nonsense_sector_class,
            SectorId::from(0),
            temp_dir.clone(),
            [0u8; 32],
            temp_dir.clone(),
            temp_dir.clone(),
            temp_dir,
            1,
            2,
        )
        .await;

        assert!(result.is_err());
    }
}
