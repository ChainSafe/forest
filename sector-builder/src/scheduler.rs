use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use filecoin_proofs::error::ExpectWithBacktrace;
use storage_proofs::sector::SectorId;

use crate::error::Result;
use crate::kv_store::KeyValueStore;
use crate::metadata::{SealStatus, StagedSectorMetadata};
use crate::scheduler::SchedulerTask::{OnSealCommitComplete, OnSealPreCommitComplete};
use crate::worker::WorkerTask;
use crate::{
    CommitMode, GetSealedSectorResult, PreCommitMode, SealSeed, SealTicket, SealedSectorMetadata,
    SecondsSinceEpoch, SectorMetadataManager, UnpaddedBytesAmount,
};
use std::io::Read;

const FATAL_NORECV: &str = "could not receive task";
const FATAL_NOSEND: &str = "could not send";

pub struct Scheduler {
    pub thread: Option<thread::JoinHandle<()>>,
}

#[derive(Debug)]
pub struct PerformHealthCheck(pub bool);

#[derive(Debug)]
pub struct SealPreCommitResult {
    pub proofs_api_call_result: Result<filecoin_proofs::SealPreCommitOutput>,
    pub sector_id: SectorId,
}

#[derive(Debug)]
pub struct SealCommitResult {
    pub proofs_api_call_result: Result<filecoin_proofs::SealCommitOutput>,
    pub sector_id: SectorId,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum SchedulerTask<T: Read + Send> {
    AddPiece(
        String,
        u64,
        T,
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
    ResumeSealPreCommit(SectorId, mpsc::SyncSender<Result<()>>),
    ResumeSealCommit(SectorId, mpsc::SyncSender<Result<SealedSectorMetadata>>),
    SealPreCommit(SectorId, SealTicket, mpsc::SyncSender<Result<()>>),
    SealCommit(
        SectorId,
        SealSeed,
        mpsc::SyncSender<Result<SealedSectorMetadata>>,
    ),
    OnSealPreCommitComplete(SealPreCommitResult, mpsc::SyncSender<Result<()>>),
    OnSealCommitComplete(
        SealCommitResult,
        mpsc::SyncSender<Result<SealedSectorMetadata>>,
    ),
    OnRetrievePieceComplete(
        Result<(UnpaddedBytesAmount, PathBuf)>,
        mpsc::SyncSender<Result<Vec<u8>>>,
    ),
    Shutdown,
}

impl<T: Read + Send> SchedulerTask<T> {
    fn should_continue(&self) -> bool {
        match self {
            SchedulerTask::Shutdown => false,
            _ => true,
        }
    }
}

struct TaskHandler<T: KeyValueStore, V: 'static + Send + std::io::Read> {
    m: SectorMetadataManager<T>,
    scheduler_tx: mpsc::SyncSender<SchedulerTask<V>>,
    worker_tx: mpsc::Sender<WorkerTask>,
}

impl<T: KeyValueStore, V: 'static + Send + std::io::Read> TaskHandler<T, V> {
    // the handle method processes a single scheduler task, returning false when
    // it has processed the shutdown task
    fn handle(&mut self, task: SchedulerTask<V>) -> bool {
        let should_continue = task.should_continue();

        match task {
            SchedulerTask::AddPiece(key, amt, file, store_until, tx) => {
                match self.m.add_piece(key, amt, file, store_until) {
                    Ok(sector_id) => {
                        tx.send(Ok(sector_id)).expects(FATAL_NOSEND);
                    }
                    Err(err) => {
                        tx.send(Err(err)).expects(FATAL_NOSEND);
                    }
                }
            }
            SchedulerTask::GetSealStatus(sector_id, tx) => {
                tx.send(self.m.get_seal_status(sector_id))
                    .expects(FATAL_NOSEND);
            }
            SchedulerTask::RetrievePiece(piece_key, tx) => {
                match self.m.create_retrieve_piece_task_proto(piece_key) {
                    Ok(proto) => {
                        let scheduler_tx_c = self.scheduler_tx.clone();

                        let callback = Box::new(move |output| {
                            scheduler_tx_c
                                .send(SchedulerTask::OnRetrievePieceComplete(output, tx))
                                .expects(FATAL_NOSEND)
                        });

                        self.worker_tx
                            .send(WorkerTask::Unseal {
                                comm_d: proto.comm_d,
                                destination_path: proto.destination_path,
                                piece_len: proto.piece_len,
                                piece_start_byte: proto.piece_start_byte,
                                porep_config: proto.porep_config,
                                seal_ticket: proto.seal_ticket,
                                sector_id: proto.sector_id,
                                source_path: proto.source_path,
                                callback,
                            })
                            .expects(FATAL_NOSEND);
                    }
                    Err(err) => {
                        tx.send(Err(err)).expects(FATAL_NOSEND);
                    }
                }
            }
            SchedulerTask::GetSealedSectors(check_health, tx) => {
                tx.send(self.m.get_sealed_sectors_filtered(check_health.0, |_| true))
                    .expects(FATAL_NOSEND);
            }
            SchedulerTask::GetStagedSectors(tx) => {
                tx.send(Ok(self
                    .m
                    .get_staged_sectors_filtered(|_| true)
                    .into_iter()
                    .cloned()
                    .collect()))
                    .expect(FATAL_NOSEND);
            }
            SchedulerTask::SealPreCommit(sector_id, t, tx) => {
                self.send_pre_commit_to_worker(sector_id, PreCommitMode::StartFresh(t), tx);
            }
            SchedulerTask::ResumeSealPreCommit(sector_id, tx) => {
                self.send_pre_commit_to_worker(sector_id, PreCommitMode::Resume, tx);
            }
            SchedulerTask::SealCommit(sector_id, seed, tx) => {
                self.send_commit_to_worker(sector_id, CommitMode::StartFresh(seed), tx);
            }
            SchedulerTask::ResumeSealCommit(sector_id, tx) => {
                self.send_commit_to_worker(sector_id, CommitMode::Resume, tx);
            }
            SchedulerTask::OnSealPreCommitComplete(output, done_tx) => {
                done_tx
                    .send(self.m.handle_seal_pre_commit_result(output))
                    .expects(FATAL_NOSEND);
            }
            SchedulerTask::OnSealCommitComplete(output, done_tx) => {
                done_tx
                    .send(self.m.handle_seal_commit_result(output))
                    .expects(FATAL_NOSEND);
            }

            SchedulerTask::OnRetrievePieceComplete(result, tx) => {
                tx.send(self.m.read_unsealed_bytes_from(result))
                    .expects(FATAL_NOSEND);
            }
            SchedulerTask::GeneratePoSt(comm_rs, chg_seed, faults, tx) => {
                let proto = self
                    .m
                    .create_generate_post_task_proto(&comm_rs, &chg_seed, faults);

                let tx_c = tx.clone();

                let callback = Box::new(move |r| tx_c.send(r).expects(FATAL_NOSEND));

                self.worker_tx
                    .send(WorkerTask::GeneratePoSt {
                        challenge_seed: proto.challenge_seed,
                        private_replicas: proto.private_replicas,
                        post_config: proto.post_config,
                        callback,
                    })
                    .expects(FATAL_NOSEND);
            }
            SchedulerTask::Shutdown => (),
        };

        should_continue
    }

    // Creates and sends a commit task to a worker. If the requested sector
    // id and mode combination are invalid, done_tx receives an error.
    fn send_commit_to_worker(
        &mut self,
        sector_id: SectorId,
        mode: CommitMode,
        done_tx: mpsc::SyncSender<Result<SealedSectorMetadata>>,
    ) {
        let scheduler_tx_c = self.scheduler_tx.clone();

        let done_tx_c = done_tx.clone();

        let callback = Box::new(move |output| {
            scheduler_tx_c
                .send(OnSealCommitComplete(output, done_tx_c))
                .expects(FATAL_NOSEND)
        });

        match self.m.create_seal_commit_task_proto(sector_id, mode) {
            Ok(proto) => {
                self.worker_tx
                    .send(WorkerTask::SealCommit {
                        cache_dir: proto.cache_dir,
                        callback,
                        piece_info: proto.piece_info,
                        porep_config: proto.porep_config,
                        pre_commit: proto.pre_commit,
                        sector_id: proto.sector_id,
                        seed: proto.seed,
                        ticket: proto.ticket,
                    })
                    .expects(FATAL_NOSEND);
            }
            Err(err) => done_tx.send(Err(err)).expects(FATAL_NOSEND),
        }
    }

    // Creates and sends a pre-commit task to a worker. If the requested sector
    // id and mode combination are invalid, done_tx receives an error.
    fn send_pre_commit_to_worker(
        &mut self,
        sector_id: SectorId,
        mode: PreCommitMode,
        done_tx: mpsc::SyncSender<Result<()>>,
    ) {
        let scheduler_tx_c = self.scheduler_tx.clone();

        let done_tx_c = done_tx.clone();

        let callback = Box::new(move |output| {
            scheduler_tx_c
                .send(OnSealPreCommitComplete(output, done_tx_c))
                .expects(FATAL_NOSEND)
        });

        match self.m.create_seal_pre_commit_task_proto(sector_id, mode) {
            Ok(proto) => {
                self.worker_tx
                    .send(WorkerTask::SealPreCommit {
                        cache_dir: proto.cache_dir,
                        callback,
                        piece_info: proto.piece_info,
                        porep_config: proto.porep_config,
                        sealed_sector_path: proto.sealed_sector_path,
                        sector_id: proto.sector_id,
                        staged_sector_path: proto.staged_sector_path,
                        ticket: proto.ticket,
                    })
                    .expects(FATAL_NOSEND);
            }
            Err(err) => done_tx.send(Err(err)).expects(FATAL_NOSEND),
        }
    }
}

impl Scheduler {
    #[allow(clippy::too_many_arguments)]
    pub fn start<T: 'static + KeyValueStore, U: 'static + std::io::Read + Send>(
        scheduler_tx: mpsc::SyncSender<SchedulerTask<U>>,
        scheduler_rx: mpsc::Receiver<SchedulerTask<U>>,
        worker_tx: mpsc::Sender<WorkerTask>,
        m: SectorMetadataManager<T>,
    ) -> Result<Scheduler> {
        let thread = thread::spawn(move || {
            let mut h = TaskHandler {
                m,
                scheduler_tx,
                worker_tx: worker_tx.clone(),
            };

            loop {
                let task = scheduler_rx.recv().expects(FATAL_NORECV);
                if !h.handle(task) {
                    break;
                }
            }
        });

        Ok(Scheduler {
            thread: Some(thread),
        })
    }
}
