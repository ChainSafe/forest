use std::sync::{mpsc, Arc, Mutex};

use filecoin_proofs::error::ExpectWithBacktrace;
use filecoin_proofs::post_adapter::*;
use filecoin_proofs::types::{PaddedBytesAmount, SectorClass};

use crate::constants::*;
use crate::disk_backed_storage::new_sector_store;
use crate::error::{Result, SectorBuilderErr};
use crate::kv_store::{KeyValueStore, SledKvs};
use crate::metadata::*;
use crate::scheduler::{Request, Scheduler};
use crate::sealer::*;

pub type SectorId = u64;

pub struct SectorBuilder {
    // Prevents FFI consumers from queueing behind long-running seal operations.
    sealers_tx: mpsc::Sender<SealerInput>,

    // For additional seal concurrency, add more workers here.
    sealers: Vec<SealerWorker>,

    // The main worker's queue.
    scheduler_tx: mpsc::SyncSender<Request>,

    // The main worker. Owns all mutable state for the SectorBuilder.
    scheduler: Scheduler,

    // Configures size of proofs and sectors managed by the SectorBuilder.
    sector_class: SectorClass,
}

impl SectorBuilder {
    // Initialize and return a SectorBuilder from metadata persisted to disk if
    // it exists. Otherwise, initialize and return a fresh SectorBuilder. The
    // metadata key is equal to the prover_id.
    pub fn init_from_metadata<S: Into<String>>(
        sector_class: SectorClass,
        last_committed_sector_id: SectorId,
        metadata_dir: S,
        prover_id: [u8; 31],
        sealed_sector_dir: S,
        staged_sector_dir: S,
        max_num_staged_sectors: u8,
    ) -> Result<SectorBuilder> {
        let kv_store = Arc::new(WrappedKeyValueStore {
            inner: Box::new(SledKvs::initialize(metadata_dir.into())?),
        });

        // Initialize a SectorStore and wrap it in an Arc so we can access it
        // from multiple threads. Our implementation assumes that the
        // SectorStore is safe for concurrent access.
        let sector_store = Arc::new(new_sector_store(
            sector_class,
            sealed_sector_dir.into(),
            staged_sector_dir.into(),
        ));

        // Configure the main worker's rendezvous channel.
        let (main_tx, main_rx) = mpsc::sync_channel(0);

        // Configure seal queue workers and channels.
        let (seal_tx, seal_workers) = {
            let (tx, rx) = mpsc::channel();
            let rx = Arc::new(Mutex::new(rx));

            let workers = (0..NUM_SEAL_WORKERS)
                .map(|n| SealerWorker::start(n, rx.clone(), sector_store.clone(), prover_id))
                .collect();

            (tx, workers)
        };

        let SectorClass(sector_size, _, _) = sector_class;

        // Configure main worker.
        let main_worker = Scheduler::start_with_metadata(
            main_rx,
            main_tx.clone(),
            seal_tx.clone(),
            kv_store.clone(),
            sector_store.clone(),
            last_committed_sector_id,
            max_num_staged_sectors,
            prover_id,
            PaddedBytesAmount::from(sector_size),
        );

        Ok(SectorBuilder {
            scheduler_tx: main_tx,
            scheduler: main_worker,
            sealers_tx: seal_tx,
            sealers: seal_workers,
            sector_class,
        })
    }

    // Stages user piece-bytes for sealing. Note that add_piece calls are
    // processed sequentially to make bin packing easier.
    pub fn add_piece(
        &self,
        piece_key: String,
        piece_bytes_amount: u64,
        piece_path: String,
        store_until: SecondsSinceEpoch,
    ) -> Result<SectorId> {
        log_unrecov(self.run_blocking(|tx| {
            Request::AddPiece(piece_key, piece_bytes_amount, piece_path, store_until, tx)
        }))
    }

    // Returns sealing status for the sector with specified id. If no sealed or
    // staged sector exists with the provided id, produce an error.
    pub fn get_seal_status(&self, sector_id: SectorId) -> Result<SealStatus> {
        log_unrecov(self.run_blocking(|tx| Request::GetSealStatus(sector_id, tx)))
    }

    // Unseals the sector containing the referenced piece and returns its
    // bytes. Produces an error if this sector builder does not have a sealed
    // sector containing the referenced piece.
    pub fn read_piece_from_sealed_sector(&self, piece_key: String) -> Result<Vec<u8>> {
        log_unrecov(self.run_blocking(|tx| Request::RetrievePiece(piece_key, tx)))
    }

    // For demo purposes. Schedules sealing of all staged sectors.
    pub fn seal_all_staged_sectors(&self) -> Result<()> {
        log_unrecov(self.run_blocking(Request::SealAllStagedSectors))
    }

    // Returns all sealed sector metadata.
    pub fn get_sealed_sectors(&self) -> Result<Vec<SealedSectorMetadata>> {
        log_unrecov(self.run_blocking(Request::GetSealedSectors))
    }

    // Returns all staged sector metadata.
    pub fn get_staged_sectors(&self) -> Result<Vec<StagedSectorMetadata>> {
        log_unrecov(self.run_blocking(Request::GetStagedSectors))
    }

    // Generates a proof-of-spacetime. Blocks the calling thread.
    pub fn generate_post(
        &self,
        comm_rs: &[[u8; 32]],
        challenge_seed: &[u8; 32],
    ) -> Result<GeneratePoStDynamicSectorsCountOutput> {
        log_unrecov(
            self.run_blocking(|tx| Request::GeneratePoSt(Vec::from(comm_rs), *challenge_seed, tx)),
        )
    }

    // Run a task, blocking on the return channel.
    fn run_blocking<T, F: FnOnce(mpsc::SyncSender<T>) -> Request>(&self, with_sender: F) -> T {
        let (tx, rx) = mpsc::sync_channel(0);

        self.scheduler_tx
            .clone()
            .send(with_sender(tx))
            .expects(FATAL_NOSEND_TASK);

        rx.recv().expects(FATAL_NORECV_TASK)
    }

    // Return the SectorBuilder's configured sector class.
    pub fn get_sector_class(&self) -> SectorClass {
        self.sector_class
    }
}

impl Drop for SectorBuilder {
    fn drop(&mut self) {
        // Shut down main worker and sealers, too.
        let _ = self
            .scheduler_tx
            .send(Request::Shutdown)
            .map_err(|err| println!("err sending Shutdown to scheduler: {:?}", err));

        for _ in &mut self.sealers {
            let _ = self
                .sealers_tx
                .send(SealerInput::Shutdown)
                .map_err(|err| println!("err sending Shutdown to sealer: {:?}", err));
        }

        // Wait for worker threads to return.
        let scheduler_thread = &mut self.scheduler.thread;

        if let Some(thread) = scheduler_thread.take() {
            let _ = thread
                .join()
                .map_err(|err| println!("err joining scheduler thread: {:?}", err));
        }

        for worker in &mut self.sealers {
            if let Some(thread) = worker.thread.take() {
                let _ = thread
                    .join()
                    .map_err(|err| println!("err joining sealer thread: {:?}", err));
            }
        }
    }
}

pub struct WrappedKeyValueStore<T: KeyValueStore> {
    inner: Box<T>,
}
impl<T: KeyValueStore> WrappedKeyValueStore<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner: Box::new(inner),
        }
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }
}

fn log_unrecov<T>(result: Result<T>) -> Result<T> {
    if let Err(err) = &result {
        if let Some(SectorBuilderErr::Unrecoverable(err, backtrace)) = err.downcast_ref() {
            error!("unrecoverable: {:?} - {:?}", err, backtrace);
        }
    }

    result
}
