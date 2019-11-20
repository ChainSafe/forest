use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use filecoin_proofs::error::ExpectWithBacktrace;

use crate::error::Result;
use crate::scheduler::{SealCommitResult, SealPreCommitResult};
use crate::{PoRepConfig, SealSeed, SealTicket, UnpaddedByteIndex, UnpaddedBytesAmount};
use filecoin_proofs::{Candidate, PieceInfo, PoStConfig, PrivateReplicaInfo, SealPreCommitOutput};
use std::collections::btree_map::BTreeMap;
use std::path::PathBuf;
use storage_proofs::sector::SectorId;

const FATAL_NOLOCK: &str = "error acquiring task lock";
const FATAL_RCVTSK: &str = "error receiving task";

pub struct Worker {
    pub id: u8,
    pub thread: Option<thread::JoinHandle<()>>,
}

pub struct UnsealTaskPrototype {
    pub(crate) comm_d: [u8; 32],
    pub(crate) cache_dir: PathBuf,
    pub(crate) destination_path: PathBuf,
    pub(crate) piece_len: UnpaddedBytesAmount,
    pub(crate) piece_start_byte: UnpaddedByteIndex,
    pub(crate) porep_config: PoRepConfig,
    pub(crate) seal_ticket: SealTicket,
    pub(crate) sector_id: SectorId,
    pub(crate) source_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct GeneratePoStTaskPrototype {
    pub(crate) randomness: [u8; 32],
    pub(crate) post_config: PoStConfig,
    pub(crate) private_replicas: BTreeMap<SectorId, PrivateReplicaInfo>,
}

#[derive(Debug)]
pub struct SealPreCommitTaskPrototype {
    pub(crate) cache_dir: PathBuf,
    pub(crate) piece_info: Vec<PieceInfo>,
    pub(crate) porep_config: PoRepConfig,
    pub(crate) sealed_sector_path: PathBuf,
    pub(crate) sector_id: SectorId,
    pub(crate) staged_sector_path: PathBuf,
    pub(crate) ticket: SealTicket,
}

#[derive(Debug)]
pub struct SealCommitTaskPrototype {
    pub(crate) cache_dir: PathBuf,
    pub(crate) piece_info: Vec<PieceInfo>,
    pub(crate) porep_config: PoRepConfig,
    pub(crate) pre_commit: SealPreCommitOutput,
    pub(crate) sector_id: SectorId,
    pub(crate) seed: SealSeed,
    pub(crate) ticket: SealTicket,
}

type UnsealCallback = Box<dyn FnOnce(Result<(UnpaddedBytesAmount, PathBuf)>) + Send>;

type GenerateCandidatesCallback = Box<dyn FnOnce(Result<Vec<Candidate>>) + Send>;

type GeneratePoStCallback = Box<dyn FnOnce(Result<Vec<Vec<u8>>>) + Send>;

type SealPreCommitCallback = Box<dyn FnOnce(SealPreCommitResult) + Send>;

type SealCommitCallback = Box<dyn FnOnce(SealCommitResult) + Send>;

#[allow(clippy::large_enum_variant)]
pub enum WorkerTask {
    GenerateCandidates {
        randomness: [u8; 32],
        private_replicas: BTreeMap<SectorId, PrivateReplicaInfo>,
        post_config: PoStConfig,
        callback: GenerateCandidatesCallback,
    },
    GeneratePoSt {
        randomness: [u8; 32],
        private_replicas: BTreeMap<SectorId, PrivateReplicaInfo>,
        post_config: PoStConfig,
        callback: GeneratePoStCallback,
        winners: Vec<Candidate>,
    },
    SealPreCommit {
        cache_dir: PathBuf,
        callback: SealPreCommitCallback,
        piece_info: Vec<PieceInfo>,
        porep_config: PoRepConfig,
        sealed_sector_path: PathBuf,
        sector_id: SectorId,
        staged_sector_path: PathBuf,
        ticket: SealTicket,
    },
    SealCommit {
        cache_dir: PathBuf,
        callback: SealCommitCallback,
        piece_info: Vec<PieceInfo>,
        porep_config: PoRepConfig,
        pre_commit: SealPreCommitOutput,
        sector_id: SectorId,
        seed: SealSeed,
        ticket: SealTicket,
    },
    Unseal {
        comm_d: [u8; 32],
        cache_dir: PathBuf,
        destination_path: PathBuf,
        piece_len: UnpaddedBytesAmount,
        piece_start_byte: UnpaddedByteIndex,
        porep_config: PoRepConfig,
        seal_ticket: SealTicket,
        sector_id: SectorId,
        source_path: PathBuf,
        callback: UnsealCallback,
    },
    Shutdown,
}

impl Worker {
    pub fn start(
        id: u8,
        seal_task_rx: Arc<Mutex<mpsc::Receiver<WorkerTask>>>,
        prover_id: [u8; 32],
    ) -> Worker {
        let thread = thread::spawn(move || loop {
            // Acquire a lock on the rx end of the channel, get a task,
            // relinquish the lock and return the task. The receiver is mutexed
            // for coordinating reads across multiple worker-threads.
            let task = {
                let rx = seal_task_rx.lock().expects(FATAL_NOLOCK);
                rx.recv().expects(FATAL_RCVTSK)
            };

            // Dispatch to the appropriate task-handler.
            match task {
                WorkerTask::GenerateCandidates {
                    randomness,
                    private_replicas,
                    post_config,
                    callback,
                } => {
                    callback(filecoin_proofs::generate_candidates(
                        post_config,
                        &randomness,
                        &private_replicas,
                        prover_id,
                    ));
                }
                WorkerTask::GeneratePoSt {
                    randomness,
                    private_replicas,
                    post_config,
                    winners,
                    callback,
                } => {
                    callback(filecoin_proofs::generate_post(
                        post_config,
                        &randomness,
                        &private_replicas,
                        winners,
                        prover_id,
                    ));
                }
                WorkerTask::SealPreCommit {
                    cache_dir,
                    callback,
                    piece_info,
                    porep_config,
                    sealed_sector_path,
                    sector_id,
                    staged_sector_path,
                    ticket,
                } => {
                    let result = filecoin_proofs::seal_pre_commit(
                        porep_config,
                        &cache_dir,
                        &staged_sector_path,
                        &sealed_sector_path,
                        prover_id,
                        sector_id,
                        ticket.ticket_bytes,
                        &piece_info,
                    );

                    callback(SealPreCommitResult {
                        sector_id,
                        proofs_api_call_result: result,
                    });
                }
                WorkerTask::SealCommit {
                    cache_dir,
                    callback,
                    piece_info,
                    porep_config,
                    pre_commit,
                    sector_id,
                    seed,
                    ticket,
                } => {
                    let result = filecoin_proofs::seal_commit(
                        porep_config,
                        cache_dir,
                        prover_id,
                        sector_id,
                        ticket.ticket_bytes,
                        seed.ticket_bytes,
                        pre_commit,
                        &piece_info,
                    );

                    callback(SealCommitResult {
                        proofs_api_call_result: result,
                        sector_id,
                    });
                }
                WorkerTask::Unseal {
                    comm_d,
                    cache_dir,
                    destination_path,
                    piece_len,
                    piece_start_byte,
                    porep_config,
                    seal_ticket,
                    sector_id,
                    source_path,
                    callback,
                } => {
                    let result = filecoin_proofs::get_unsealed_range(
                        porep_config,
                        &cache_dir,
                        &source_path,
                        &destination_path,
                        prover_id,
                        sector_id,
                        comm_d,
                        seal_ticket.ticket_bytes,
                        piece_start_byte,
                        piece_len,
                    )
                    .map(|num_bytes_unsealed| (num_bytes_unsealed, destination_path));

                    callback(result);
                }
                WorkerTask::Shutdown => break,
            }
        });

        Worker {
            id,
            thread: Some(thread),
        }
    }
}
