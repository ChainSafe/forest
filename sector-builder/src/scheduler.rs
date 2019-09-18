use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use filecoin_proofs::error::ExpectWithBacktrace;
use filecoin_proofs::SealOutput;
use storage_proofs::sector::SectorId;

use crate::error::Result;
use crate::kv_store::KeyValueStore;
use crate::metadata::{SealStatus, StagedSectorMetadata};
use crate::store::SectorStore;
use crate::worker::WorkerTask;
use crate::{GetSealedSectorResult, SecondsSinceEpoch, SectorMetadataManager, UnpaddedBytesAmount};

const FATAL_NORECV: &str = "could not receive task";
const FATAL_NOSEND: &str = "could not send";

pub struct Scheduler {
    pub thread: Option<thread::JoinHandle<()>>,
}

#[derive(Debug)]
pub struct PerformHealthCheck(pub bool);

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum SchedulerTask {
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
    HandleSealResult(SectorId, String, PathBuf, Result<SealOutput>),
    HandleRetrievePieceResult(
        Result<(UnpaddedBytesAmount, PathBuf)>,
        mpsc::SyncSender<Result<Vec<u8>>>,
    ),
    Shutdown,
}

impl Scheduler {
    #[allow(clippy::too_many_arguments)]
    pub fn start<T: 'static + KeyValueStore, S: 'static + SectorStore>(
        scheduler_tx: mpsc::SyncSender<SchedulerTask>,
        scheduler_rx: mpsc::Receiver<SchedulerTask>,
        worker_tx: mpsc::Sender<WorkerTask>,
        mut m: SectorMetadataManager<T, S>,
    ) -> Scheduler {
        let thread = thread::spawn(move || {
            loop {
                let task = scheduler_rx.recv().expects(FATAL_NORECV);

                // Dispatch to the appropriate task-handler.
                match task {
                    SchedulerTask::AddPiece(key, amt, path, store_until, tx) => {
                        match m.add_piece(key, amt, path, store_until) {
                            Ok((sector_id, protos)) => {
                                for p in protos {
                                    worker_tx
                                        .send(WorkerTask::from_seal_proto(p, scheduler_tx.clone()))
                                        .expects(FATAL_NOSEND);
                                }

                                tx.send(Ok(sector_id)).expects(FATAL_NOSEND);
                            }
                            Err(err) => {
                                tx.send(Err(err)).expects(FATAL_NOSEND);
                            }
                        }
                    }
                    SchedulerTask::GetSealStatus(sector_id, tx) => {
                        tx.send(m.get_seal_status(sector_id)).expects(FATAL_NOSEND);
                    }
                    SchedulerTask::RetrievePiece(piece_key, tx) => {
                        match m.create_retrieve_piece_task_proto(piece_key) {
                            Ok(proto) => {
                                worker_tx
                                    .send(WorkerTask::from_unseal_proto(
                                        proto,
                                        tx.clone(),
                                        scheduler_tx.clone(),
                                    ))
                                    .expects(FATAL_NOSEND);
                            }
                            Err(err) => {
                                tx.send(Err(err)).expects(FATAL_NOSEND);
                            }
                        }
                    }
                    SchedulerTask::GetSealedSectors(check_health, tx) => {
                        tx.send(m.get_sealed_sectors(check_health.0))
                            .expects(FATAL_NOSEND);
                    }
                    SchedulerTask::GetStagedSectors(tx) => {
                        tx.send(m.get_staged_sectors()).expect(FATAL_NOSEND);
                    }
                    SchedulerTask::SealAllStagedSectors(tx) => match m.seal_all_staged_sectors() {
                        Ok(protos) => {
                            for p in protos {
                                worker_tx
                                    .send(WorkerTask::from_seal_proto(p, scheduler_tx.clone()))
                                    .expects(FATAL_NOSEND);
                            }

                            tx.send(Ok(())).expects(FATAL_NOSEND);
                        }
                        Err(err) => {
                            tx.send(Err(err)).expects(FATAL_NOSEND);
                        }
                    },
                    SchedulerTask::HandleSealResult(sector_id, access, path, result) => {
                        m.handle_seal_result(sector_id, access, path, result);
                    }
                    SchedulerTask::HandleRetrievePieceResult(result, tx) => {
                        tx.send(m.read_unsealed_bytes_from(result))
                            .expects(FATAL_NOSEND);
                    }
                    SchedulerTask::GeneratePoSt(comm_rs, chg_seed, faults, tx) => {
                        tx.send(m.generate_post(&comm_rs, &chg_seed, faults))
                            .expects(FATAL_NOSEND);
                    }
                    SchedulerTask::Shutdown => break,
                }
            }
        });

        Scheduler {
            thread: Some(thread),
        }
    }
}
