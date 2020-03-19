use std::collections::btree_map::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use async_std::sync::Receiver;

use filecoin_proofs_api::{
    post, seal, seal::SealPreCommitPhase2Output, Candidate, PieceInfo, PrivateReplicaInfo,
    RegisteredPoStProof, RegisteredSealProof, SectorId, UnpaddedByteIndex, UnpaddedBytesAmount,
};

use crate::error::Result;
use crate::scheduler::{SealCommitResult, SealPreCommitResult};
use crate::{SealSeed, SealTicket};

pub struct Worker {
    pub id: u8,
    pub thread: Option<async_std::task::JoinHandle<()>>,
}

pub struct UnsealTaskPrototype {
    pub(crate) comm_d: [u8; 32],
    pub(crate) cache_dir: PathBuf,
    pub(crate) destination_path: PathBuf,
    pub(crate) piece_len: UnpaddedBytesAmount,
    pub(crate) piece_start_byte: UnpaddedByteIndex,
    pub(crate) seal_ticket: SealTicket,
    pub(crate) sector_id: SectorId,
    pub(crate) source_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct GeneratePoStTaskPrototype {
    pub(crate) randomness: [u8; 32],
    pub(crate) challenge_count: u64,
    pub(crate) registered_proof: RegisteredPoStProof,
    pub(crate) private_replicas: BTreeMap<SectorId, PrivateReplicaInfo>,
}

#[derive(Debug)]
pub struct SealPreCommitTaskPrototype {
    pub(crate) cache_dir: PathBuf,
    pub(crate) piece_info: Vec<PieceInfo>,
    pub(crate) registered_proof: RegisteredSealProof,
    pub(crate) sealed_sector_path: PathBuf,
    pub(crate) sector_id: SectorId,
    pub(crate) staged_sector_path: PathBuf,
    pub(crate) ticket: SealTicket,
}

#[derive(Debug)]
pub struct SealCommitTaskPrototype {
    pub(crate) cache_dir: PathBuf,
    pub(crate) sealed_sector_path: PathBuf,
    pub(crate) piece_info: Vec<PieceInfo>,
    pub(crate) registered_proof: RegisteredSealProof,
    pub(crate) pre_commit: SealPreCommitPhase2Output,
    pub(crate) sector_id: SectorId,
    pub(crate) seed: SealSeed,
    pub(crate) ticket: SealTicket,
}

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + 'static + Send>>;

pub(crate) type UnsealCallback =
    Box<dyn FnOnce(Result<(UnpaddedBytesAmount, PathBuf)>) -> BoxFuture<()> + Send>;

pub(crate) type GenerateCandidatesCallback =
    Box<dyn FnOnce(Result<Vec<Candidate>>) -> BoxFuture<()> + Send>;
pub(crate) type GeneratePoStCallback =
    Box<dyn FnOnce(Result<Vec<(RegisteredPoStProof, Vec<u8>)>>) -> BoxFuture<()> + Send>;
pub(crate) type SealPreCommitCallback =
    Box<dyn FnOnce(SealPreCommitResult) -> BoxFuture<()> + Send>;
pub(crate) type SealCommitCallback = Box<dyn FnOnce(SealCommitResult) -> BoxFuture<()> + Send>;

#[allow(clippy::large_enum_variant)]
pub enum WorkerTask {
    GenerateCandidates {
        randomness: [u8; 32],
        challenge_count: u64,
        private_replicas: BTreeMap<SectorId, PrivateReplicaInfo>,
        callback: GenerateCandidatesCallback,
    },
    GeneratePoSt {
        randomness: [u8; 32],
        private_replicas: BTreeMap<SectorId, PrivateReplicaInfo>,
        callback: GeneratePoStCallback,
        winners: Vec<Candidate>,
    },
    SealPreCommit {
        registered_proof: RegisteredSealProof,
        cache_dir: PathBuf,
        callback: SealPreCommitCallback,
        piece_info: Vec<PieceInfo>,
        sealed_sector_path: PathBuf,
        sector_id: SectorId,
        staged_sector_path: PathBuf,
        ticket: SealTicket,
    },
    SealCommit {
        cache_dir: PathBuf,
        sealed_sector_path: PathBuf,
        callback: SealCommitCallback,
        piece_info: Vec<PieceInfo>,
        pre_commit: SealPreCommitPhase2Output,
        sector_id: SectorId,
        seed: SealSeed,
        ticket: SealTicket,
    },
    Unseal {
        registered_proof: RegisteredSealProof,
        comm_d: [u8; 32],
        cache_dir: PathBuf,
        destination_path: PathBuf,
        piece_len: UnpaddedBytesAmount,
        piece_start_byte: UnpaddedByteIndex,
        seal_ticket: SealTicket,
        sector_id: SectorId,
        source_path: PathBuf,
        callback: UnsealCallback,
    },
    Shutdown,
}

impl Worker {
    pub fn start(id: u8, seal_task_rx: Receiver<WorkerTask>, prover_id: [u8; 32]) -> Worker {
        let thread = async_std::task::spawn(async move {
            while let Some(task) = seal_task_rx.recv().await {
                // Dispatch to the appropriate task-handler.
                match task {
                    WorkerTask::GenerateCandidates {
                        randomness,
                        challenge_count,
                        private_replicas,
                        callback,
                    } => {
                        callback(post::generate_candidates(
                            &randomness,
                            challenge_count,
                            &private_replicas,
                            prover_id,
                        ))
                        .await;
                    }
                    WorkerTask::GeneratePoSt {
                        randomness,
                        private_replicas,
                        winners,
                        callback,
                    } => {
                        callback(post::generate_post(
                            &randomness,
                            &private_replicas,
                            winners,
                            prover_id,
                        ))
                        .await;
                    }
                    WorkerTask::SealPreCommit {
                        registered_proof,
                        cache_dir,
                        callback,
                        piece_info,
                        sealed_sector_path,
                        sector_id,
                        staged_sector_path,
                        ticket,
                    } => {
                        // TODO: make two different task.

                        let result = seal::seal_pre_commit_phase1(
                            registered_proof,
                            &cache_dir,
                            &staged_sector_path,
                            &sealed_sector_path,
                            prover_id,
                            sector_id,
                            ticket.ticket_bytes,
                            &piece_info,
                        )
                        .and_then(|result1| {
                            seal::seal_pre_commit_phase2(result1, &cache_dir, &sealed_sector_path)
                        })
                        .map_err(Into::into);

                        callback(SealPreCommitResult {
                            sector_id,
                            proofs_api_call_result: result,
                        })
                        .await;
                    }
                    WorkerTask::SealCommit {
                        cache_dir,
                        sealed_sector_path,
                        callback,
                        piece_info,
                        pre_commit,
                        sector_id,
                        seed,
                        ticket,
                    } => {
                        let result = seal::seal_commit_phase1(
                            &cache_dir,
                            &sealed_sector_path,
                            prover_id,
                            sector_id,
                            ticket.ticket_bytes,
                            seed.ticket_bytes,
                            pre_commit,
                            &piece_info,
                        )
                        .and_then(|result1| {
                            seal::clear_cache(&cache_dir)?;
                            Ok(result1)
                        })
                        .and_then(|result1| seal::seal_commit_phase2(result1, prover_id, sector_id))
                        .map_err(Into::into);

                        callback(SealCommitResult {
                            proofs_api_call_result: result,
                            sector_id,
                        })
                        .await;
                    }
                    WorkerTask::Unseal {
                        registered_proof,
                        comm_d,
                        cache_dir,
                        destination_path,
                        piece_len,
                        piece_start_byte,
                        seal_ticket,
                        sector_id,
                        source_path,
                        callback,
                    } => {
                        let result = seal::get_unsealed_range(
                            registered_proof,
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

                        callback(result).await;
                    }
                    WorkerTask::Shutdown => break,
                }
            }
        });

        Worker {
            id,
            thread: Some(thread),
        }
    }
}
