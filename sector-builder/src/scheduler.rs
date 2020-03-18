use std::path::PathBuf;

use async_std::sync::{Receiver, Sender};

use filecoin_proofs::Candidate;
use storage_proofs::sector::SectorId;

use crate::error::Result;
use crate::kv_store::KeyValueStore;
use crate::metadata::{SealStatus, StagedSectorMetadata};
use crate::scheduler::SchedulerTask::{OnSealCommitComplete, OnSealPreCommitComplete};
use crate::worker::WorkerTask;
use crate::{
    CommitMode, GetSealedSectorResult, PieceMetadata, PreCommitMode, SealSeed, SealTicket,
    SealedSectorMetadata, SecondsSinceEpoch, SectorMetadataManager, UnpaddedBytesAmount,
};

pub struct Scheduler {
    pub thread: Option<async_std::task::JoinHandle<()>>,
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
pub enum SchedulerTask {
    AddPiece(
        String,
        u64,
        PathBuf,
        SecondsSinceEpoch,
        Sender<Result<SectorId>>,
    ),
    GetSealedSectors(
        PerformHealthCheck,
        Sender<Result<Vec<GetSealedSectorResult>>>,
    ),
    GetStagedSectors(Sender<Result<Vec<StagedSectorMetadata>>>),
    GetSealStatus(SectorId, Sender<Result<SealStatus>>),
    GenerateCandidates(
        Vec<[u8; 32]>,
        [u8; 32],      // seed
        u64,           // challenge count
        Vec<SectorId>, // faults
        Sender<Result<Vec<Candidate>>>,
    ),
    GeneratePoSt(
        Vec<[u8; 32]>,
        [u8; 32],       // seed
        u64,            // challenge count
        Vec<Candidate>, // winners
        Sender<Result<Vec<Vec<u8>>>>,
    ),
    RetrievePiece(String, Sender<Result<Vec<u8>>>),
    ResumeSealPreCommit(SectorId, Sender<Result<StagedSectorMetadata>>),
    ResumeSealCommit(SectorId, Sender<Result<SealedSectorMetadata>>),
    SealPreCommit(SectorId, SealTicket, Sender<Result<StagedSectorMetadata>>),
    SealCommit(SectorId, SealSeed, Sender<Result<SealedSectorMetadata>>),
    AcquireSectorId(Sender<Result<SectorId>>),
    ImportSector {
        sector_id: SectorId,
        sector_cache_dir: PathBuf,
        sealed_sector: PathBuf,
        seal_ticket: SealTicket,
        seal_seed: SealSeed,
        comm_r: [u8; 32],
        comm_d: [u8; 32],
        pieces: Vec<PieceMetadata>,
        proof: Vec<u8>,
        done_tx: Sender<Result<()>>,
    },
    OnSealPreCommitComplete(SealPreCommitResult, Sender<Result<StagedSectorMetadata>>),
    OnSealCommitComplete(SealCommitResult, Sender<Result<SealedSectorMetadata>>),
    OnRetrievePieceComplete(
        Result<(UnpaddedBytesAmount, PathBuf)>,
        Sender<Result<Vec<u8>>>,
    ),
    Shutdown,
}

impl SchedulerTask {
    fn should_continue(&self) -> bool {
        match self {
            SchedulerTask::Shutdown => false,
            _ => true,
        }
    }
}

struct TaskHandler<T: KeyValueStore> {
    m: SectorMetadataManager<T>,
    scheduler_tx: Sender<SchedulerTask>,
    worker_tx: Sender<WorkerTask>,
}

impl<T: KeyValueStore> TaskHandler<T> {
    // the handle method processes a single scheduler task, returning false when
    // it has processed the shutdown task
    async fn handle(&mut self, task: SchedulerTask) -> bool {
        let should_continue = task.should_continue();

        match task {
            SchedulerTask::AddPiece(key, amt, file, store_until, tx) => {
                match self.m.add_piece(key, amt, file, store_until).await {
                    Ok(sector_id) => {
                        tx.send(Ok(sector_id)).await;
                    }
                    Err(err) => {
                        tx.send(Err(err)).await;
                    }
                }
            }
            SchedulerTask::GetSealStatus(sector_id, tx) => {
                tx.send(self.m.get_seal_status(sector_id)).await;
            }
            SchedulerTask::RetrievePiece(piece_key, tx) => {
                match self.m.create_retrieve_piece_task_proto(piece_key) {
                    Ok(proto) => {
                        let scheduler_tx_c = self.scheduler_tx.clone();

                        let callback: crate::worker::UnsealCallback = Box::new(|output| {
                            let fut = async move {
                                scheduler_tx_c
                                    .send(SchedulerTask::OnRetrievePieceComplete(output, tx))
                                    .await;
                            };

                            Box::pin(fut)
                        });

                        self.worker_tx
                            .send(WorkerTask::Unseal {
                                comm_d: proto.comm_d,
                                cache_dir: proto.cache_dir,
                                destination_path: proto.destination_path,
                                piece_len: proto.piece_len,
                                piece_start_byte: proto.piece_start_byte,
                                porep_config: proto.porep_config,
                                seal_ticket: proto.seal_ticket,
                                sector_id: proto.sector_id,
                                source_path: proto.source_path,
                                callback,
                            })
                            .await;
                    }
                    Err(err) => {
                        tx.send(Err(err)).await;
                    }
                }
            }
            SchedulerTask::GetSealedSectors(check_health, tx) => {
                tx.send(
                    self.m
                        .get_sealed_sectors_filtered(check_health.0, |_| true)
                        .await,
                )
                .await;
            }
            SchedulerTask::GetStagedSectors(tx) => {
                tx.send(Ok(self
                    .m
                    .get_staged_sectors_filtered(|_| true)
                    .into_iter()
                    .cloned()
                    .collect()))
                    .await;
            }
            SchedulerTask::SealPreCommit(sector_id, t, tx) => {
                self.send_pre_commit_to_worker(sector_id, PreCommitMode::StartFresh(t), tx)
                    .await;
            }
            SchedulerTask::ResumeSealPreCommit(sector_id, tx) => {
                self.send_pre_commit_to_worker(sector_id, PreCommitMode::Resume, tx)
                    .await;
            }
            SchedulerTask::SealCommit(sector_id, seed, tx) => {
                self.send_commit_to_worker(sector_id, CommitMode::StartFresh(seed), tx)
                    .await;
            }
            SchedulerTask::ResumeSealCommit(sector_id, tx) => {
                self.send_commit_to_worker(sector_id, CommitMode::Resume, tx)
                    .await;
            }
            SchedulerTask::OnSealPreCommitComplete(output, done_tx) => {
                done_tx
                    .send(self.m.handle_seal_pre_commit_result(output))
                    .await;
            }
            SchedulerTask::OnSealCommitComplete(output, done_tx) => {
                done_tx
                    .send(self.m.handle_seal_commit_result(output).await)
                    .await;
            }

            SchedulerTask::OnRetrievePieceComplete(result, tx) => {
                tx.send(self.m.read_unsealed_bytes_from(result)).await;
            }
            SchedulerTask::GenerateCandidates(
                comm_rs,
                challenge_seed,
                challenge_count,
                faults,
                tx,
            ) => {
                let proto = self.m.create_generate_post_task_proto(
                    &comm_rs,
                    &challenge_seed,
                    challenge_count,
                    Some(faults),
                );

                let callback: crate::worker::GenerateCandidatesCallback = Box::new(|r| {
                    let fut = async move {
                        tx.send(r).await;
                    };
                    Box::pin(fut)
                });

                self.worker_tx
                    .send(WorkerTask::GenerateCandidates {
                        randomness: proto.randomness,
                        challenge_count: proto.challenge_count,
                        private_replicas: proto.private_replicas,
                        post_config: proto.post_config,
                        callback,
                    })
                    .await;
            }
            SchedulerTask::GeneratePoSt(comm_rs, challenge_seed, challenge_count, winners, tx) => {
                let proto = self.m.create_generate_post_task_proto(
                    &comm_rs,
                    &challenge_seed,
                    challenge_count,
                    None,
                );

                let callback: crate::worker::GeneratePoStCallback = Box::new(|r| {
                    let fut = async move {
                        tx.send(r).await;
                    };
                    Box::pin(fut)
                });

                self.worker_tx
                    .send(WorkerTask::GeneratePoSt {
                        randomness: proto.randomness,
                        private_replicas: proto.private_replicas,
                        post_config: proto.post_config,
                        winners,
                        callback,
                    })
                    .await;
            }
            SchedulerTask::ImportSector {
                sector_id,
                sector_cache_dir,
                sealed_sector,
                seal_ticket,
                seal_seed,
                comm_r,
                comm_d,
                pieces,
                proof,
                done_tx,
            } => {
                done_tx
                    .send(
                        self.m
                            .import_sector(
                                sector_id,
                                sector_cache_dir,
                                sealed_sector,
                                seal_ticket,
                                seal_seed,
                                comm_r,
                                comm_d,
                                pieces,
                                proof,
                            )
                            .await,
                    )
                    .await
            }
            SchedulerTask::AcquireSectorId(tx) => {
                tx.send(Ok(self.m.acquire_sector_id())).await;
            }
            SchedulerTask::Shutdown => (),
        };

        should_continue
    }

    // Creates and sends a commit task to a worker. If the requested sector
    // id and mode combination are invalid, done_tx receives an error.
    async fn send_commit_to_worker(
        &mut self,
        sector_id: SectorId,
        mode: CommitMode,
        done_tx: Sender<Result<SealedSectorMetadata>>,
    ) {
        let scheduler_tx_c = self.scheduler_tx.clone();

        let done_tx_c = done_tx.clone();

        let callback: crate::worker::SealCommitCallback = Box::new(|output| {
            let fut = async move {
                scheduler_tx_c
                    .send(OnSealCommitComplete(output, done_tx_c))
                    .await
            };
            Box::pin(fut)
        });

        match self.m.create_seal_commit_task_proto(sector_id, mode) {
            Ok(proto) => {
                self.worker_tx
                    .send(WorkerTask::SealCommit {
                        cache_dir: proto.cache_dir,
                        sealed_sector_path: proto.sealed_sector_path,
                        callback,
                        piece_info: proto.piece_info,
                        porep_config: proto.porep_config,
                        pre_commit: proto.pre_commit,
                        sector_id: proto.sector_id,
                        seed: proto.seed,
                        ticket: proto.ticket,
                    })
                    .await;
            }
            Err(err) => done_tx.send(Err(err)).await,
        }
    }

    // Creates and sends a pre-commit task to a worker. If the requested sector
    // id and mode combination are invalid, done_tx receives an error.
    async fn send_pre_commit_to_worker(
        &mut self,
        sector_id: SectorId,
        mode: PreCommitMode,
        done_tx: Sender<Result<StagedSectorMetadata>>,
    ) {
        let scheduler_tx_c = self.scheduler_tx.clone();

        let done_tx_c = done_tx.clone();

        let callback: crate::worker::SealPreCommitCallback = Box::new(|output| {
            let fut = async move {
                scheduler_tx_c
                    .send(OnSealPreCommitComplete(output, done_tx_c))
                    .await
            };
            Box::pin(fut)
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
                    .await;
            }
            Err(err) => done_tx.send(Err(err)).await,
        }
    }
}

impl Scheduler {
    #[allow(clippy::too_many_arguments)]
    pub async fn start<T: 'static + KeyValueStore>(
        scheduler_tx: Sender<SchedulerTask>,
        scheduler_rx: Receiver<SchedulerTask>,
        worker_tx: Sender<WorkerTask>,
        m: SectorMetadataManager<T>,
    ) -> Result<Scheduler> {
        let thread = async_std::task::spawn(async move {
            let mut h = TaskHandler {
                m,
                scheduler_tx,
                worker_tx: worker_tx.clone(),
            };

            while let Some(task) = scheduler_rx.recv().await {
                if !h.handle(task).await {
                    break;
                }
            }
        });

        Ok(Scheduler {
            thread: Some(thread),
        })
    }
}
