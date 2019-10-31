use std::collections::btree_map::BTreeMap;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::PathBuf;

use filecoin_proofs::error::ExpectWithBacktrace;
use filecoin_proofs::pieces::get_piece_start_byte;
use filecoin_proofs::{
    PaddedBytesAmount, PrivateReplicaInfo, SealCommitOutput, SealPreCommitOutput, TemporaryAux,
    UnpaddedBytesAmount,
};
use serde::{Deserialize, Serialize};
use storage_proofs::sector::SectorId;

use helpers::SnapshotKey;

use crate::error::Result;
use crate::kv_store::KeyValueStore;
use crate::scheduler::{SealCommitResult, SealPreCommitResult};
use crate::state::SectorBuilderState;
use crate::worker::{
    GeneratePoStTaskPrototype, SealCommitTaskPrototype, SealPreCommitTaskPrototype,
    UnsealTaskPrototype,
};
use crate::GetSealedSectorResult::WithHealth;
use crate::{
    err_piecenotfound, err_unrecov, GetSealedSectorResult, PersistablePreCommitOutput, SealSeed,
    SealStatus, SealedSectorMetadata, SecondsSinceEpoch, SectorStore, StagedSectorMetadata,
};
use crate::{helpers, SealTicket};

const FATAL_SNPSHT: &str = "could not snapshot";

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq, Hash)]
pub struct TemporaryAuxKey(SectorId);

// The SectorBuilderStateManager is the owner of all sector-related metadata.
// It dispatches expensive operations (e.g. unseal and seal) to the sealer
// worker-threads. Other, inexpensive work (or work which needs to be performed
// serially) is handled by the SectorBuilderStateManager itself.
pub struct SectorMetadataManager<T: KeyValueStore> {
    kv_store: T,
    sector_store: SectorStore,
    state: SectorBuilderState,
    max_num_staged_sectors: u8,
    max_user_bytes_per_staged_sector: UnpaddedBytesAmount,
    prover_id: [u8; 32],
    sector_size: PaddedBytesAmount,

    /// WARNING: This map is a stopgap to help us move forward while we work on
    /// the merkle tree cache. The existing SealPreCommitOutput struct isn't
    /// serializable, so we have to hold it in memory until somebody calls
    /// commit. The sector builder will panic if asked to commit a sector for
    /// which we do not have any pre-commit output.
    temporary_aux: HashMap<TemporaryAuxKey, TemporaryAux>,
}

impl<T: KeyValueStore> SectorMetadataManager<T> {
    pub fn initialize(
        kv_store: T,
        sector_store: SectorStore,
        mut state: SectorBuilderState,
        max_num_staged_sectors: u8,
        max_user_bytes_per_staged_sector: UnpaddedBytesAmount,
        prover_id: [u8; 32],
        sector_size: PaddedBytesAmount,
    ) -> SectorMetadataManager<T> {
        // If a previous instance of the SectorBuilder was shut down mid-seal,
        // its metadata store will contain staged sectors who are still
        // sealing. If we do have any of those when we start the Scheduler,
        // we should transition them to a paused state and let the consumers
        // schedule appropriate resumption calls.
        //
        // For more information, see rust-fil-sector-builder/17.
        for ssm in state.staged.sectors.values_mut() {
            if let SealStatus::PreCommitting(ref t) = ssm.seal_status {
                ssm.seal_status = SealStatus::PreCommittingPaused(t.clone())
            } else if let SealStatus::Committing(ref t, ref k, ref p, ref s) = ssm.seal_status {
                ssm.seal_status =
                    SealStatus::CommittingPaused(t.clone(), k.clone(), p.clone(), s.clone())
            }
        }

        SectorMetadataManager {
            kv_store,
            sector_store,
            state,
            max_num_staged_sectors,
            max_user_bytes_per_staged_sector,
            prover_id,
            sector_size,
            temporary_aux: Default::default(),
        }
    }
}

pub enum PreCommitMode {
    StartFresh(SealTicket),
    Resume,
}

pub enum CommitMode {
    StartFresh(SealSeed),
    Resume,
}

impl<T: KeyValueStore> SectorMetadataManager<T> {
    pub fn create_generate_post_task_proto(
        &self,
        comm_rs: &[[u8; 32]],
        challenge_seed: &[u8; 32],
        faults: Vec<SectorId>,
    ) -> GeneratePoStTaskPrototype {
        let fault_set: HashSet<SectorId> = faults.into_iter().collect();

        let comm_rs_set: HashSet<&[u8; 32]> = comm_rs.iter().collect();

        let mut replicas: BTreeMap<SectorId, PrivateReplicaInfo> = Default::default();

        for sector in self.state.sealed.sectors.values() {
            if comm_rs_set.contains(&sector.comm_r) {
                let path_str = self
                    .sector_store
                    .manager()
                    .sealed_sector_path(&sector.sector_access)
                    .to_str()
                    .map(str::to_string)
                    .unwrap();

                let cache_dir = self
                    .sector_store
                    .manager()
                    .cache_path(&sector.sector_access);

                let info = if fault_set.contains(&sector.sector_id) {
                    PrivateReplicaInfo::new_faulty(
                        path_str,
                        sector.comm_r,
                        sector.p_aux.clone(),
                        cache_dir,
                    )
                } else {
                    PrivateReplicaInfo::new(
                        path_str,
                        sector.comm_r,
                        sector.p_aux.clone(),
                        cache_dir,
                    )
                };

                replicas.insert(sector.sector_id, info);
            }
        }

        GeneratePoStTaskPrototype {
            challenge_seed: *challenge_seed,
            post_config: self.sector_store.proofs_config().post_config,
            private_replicas: replicas,
        }
    }

    // Creates a task prototype for retrieving (unsealing) a piece from a
    // sealed sector.
    pub fn create_retrieve_piece_task_proto(
        &self,
        piece_key: String,
    ) -> Result<UnsealTaskPrototype> {
        let opt_sealed_sector = self.state.sealed.sectors.values().find(|sector| {
            sector
                .pieces
                .iter()
                .any(|piece| piece.piece_key == piece_key)
        });

        let sealed_sector =
            opt_sealed_sector.ok_or_else(|| err_piecenotfound(piece_key.to_string()))?;

        let piece = sealed_sector
            .pieces
            .iter()
            .find(|p| p.piece_key == piece_key)
            .ok_or_else(|| err_piecenotfound(piece_key.clone()))?;

        let piece_lengths: Vec<_> = sealed_sector
            .pieces
            .iter()
            .take_while(|p| p.piece_key != piece_key)
            .map(|p| p.num_bytes)
            .collect();

        let staged_sector_access = self
            .sector_store
            .manager()
            .new_staging_sector_access(sealed_sector.sector_id)
            .map_err(failure::Error::from)?;

        Ok(UnsealTaskPrototype {
            comm_d: sealed_sector.comm_d,
            porep_config: self.sector_store.proofs_config().porep_config,
            source_path: self
                .sector_store
                .manager()
                .sealed_sector_path(&sealed_sector.sector_access),
            destination_path: self
                .sector_store
                .manager()
                .staged_sector_path(&staged_sector_access),
            sector_id: sealed_sector.sector_id,
            piece_start_byte: get_piece_start_byte(&piece_lengths, piece.num_bytes),
            piece_len: piece.num_bytes,
            seal_ticket: sealed_sector.ticket.clone(),
        })
    }

    // Returns sealing status for the sector with specified id. If no sealed or
    // staged sector exists with the provided id, produce an error.
    pub fn get_seal_status(&self, sector_id: SectorId) -> Result<SealStatus> {
        helpers::get_seal_status(&self.state.staged, &self.state.sealed, sector_id)
    }

    // Write the piece to storage, obtaining the sector id with which the
    // piece-bytes are now associated and a vector of SealTaskPrototypes.
    pub fn add_piece<U: Read>(
        &mut self,
        piece_key: String,
        piece_bytes_amount: u64,
        piece_file: U,
        store_until: SecondsSinceEpoch,
    ) -> Result<SectorId> {
        let destination_sector_id = helpers::add_piece(
            &self.sector_store,
            &mut self.state,
            piece_bytes_amount,
            piece_key,
            piece_file,
            store_until,
        )?;

        self.check_and_schedule(false);
        self.checkpoint().expects(FATAL_SNPSHT);

        Ok(destination_sector_id)
    }

    // For demo purposes. Schedules sealing of all staged sectors.
    pub fn mark_all_sectors_for_sealing(&mut self) {
        self.check_and_schedule(true);
        self.checkpoint().expects(FATAL_SNPSHT);
    }

    // Produces a vector containing metadata for all sealed sectors that this
    // SectorBuilder knows about. Includes sector health-information on request.
    pub fn get_sealed_sectors_filtered<P: FnMut(&SealedSectorMetadata) -> bool>(
        &self,
        check_health: bool,
        mut predicate: P,
    ) -> Result<Vec<GetSealedSectorResult>> {
        use rayon::prelude::*;

        let sectors_iter = self
            .state
            .sealed
            .sectors
            .values()
            .filter(|x| predicate(*x))
            .cloned();

        if !check_health {
            return Ok(sectors_iter
                .map(GetSealedSectorResult::WithoutHealth)
                .collect());
        }

        let with_path: Vec<(PathBuf, SealedSectorMetadata)> = sectors_iter
            .map(|meta| {
                let pbuf = self
                    .sector_store
                    .manager()
                    .sealed_sector_path(&meta.sector_access);

                (pbuf, meta)
            })
            .collect();

        // compute sector health in parallel using workers from rayon global
        // thread pool
        with_path
            .into_par_iter()
            .map(|(pbuf, meta)| {
                let health = helpers::get_sealed_sector_health(&pbuf, &meta)?;
                Ok(WithHealth(health, meta))
            })
            .collect()
    }

    // Produces a task prototype for a staged sector with the provided id and
    // marks the sector for pre-committing.  If a seed is already associated
    // with the target sector, use it. If not, use the provided seed. If no
    // sector exists with the provided id or if the sector is not ready for
    // pre-committing, an error is returned.
    pub fn create_seal_pre_commit_task_proto(
        &mut self,
        sector_id: SectorId,
        mode: PreCommitMode,
    ) -> Result<SealPreCommitTaskPrototype> {
        let opt_meta = self
            .state
            .staged
            .sectors
            .values_mut()
            .find(|s| s.sector_id == sector_id);

        let meta =
            opt_meta.ok_or_else(|| format_err!("no staged sector with id {} exists", sector_id))?;

        let ticket = match (mode, &meta.seal_status) {
            (PreCommitMode::StartFresh(t), SealStatus::AcceptingPieces) => Ok(t),
            (PreCommitMode::StartFresh(t), SealStatus::FullyPacked) => Ok(t),
            (PreCommitMode::StartFresh(_), s) => Err(format_err!(
                "cannot pre-commit sector with id {:?} and state {:?}",
                sector_id,
                s,
            )),
            (PreCommitMode::Resume, SealStatus::PreCommittingPaused(t)) => Ok(t.clone()),
            (PreCommitMode::Resume, s) => Err(format_err!(
                "cannot resume pre-commit sector with id {:?} and state {:?}",
                sector_id,
                s,
            )),
        }?;

        let mgr = self.sector_store.manager();

        let _ = mgr.new_sealed_sector_access(sector_id)?;

        let out = Ok(SealPreCommitTaskPrototype {
            cache_dir: mgr.cache_path(&meta.sector_access),
            piece_info: meta.pieces.iter().map(|x| (x.clone()).into()).collect(),
            porep_config: self.sector_store.proofs_config().porep_config,
            sealed_sector_path: mgr.sealed_sector_path(&meta.sector_access),
            sector_id,
            staged_sector_path: mgr.staged_sector_path(&meta.sector_access),
            ticket: ticket.clone(),
        });

        meta.seal_status = SealStatus::PreCommitting(ticket);
        self.checkpoint().expects(FATAL_SNPSHT);

        out
    }

    // Produces a task prototype for a staged sector with the provided id and
    // marks the sector for committing.  If a seed is already associated with
    // the target sector, use it. If not, use the provided seed. If no sector
    // exists with the provided id or if the sector is not ready to be
    // committed, an error is returned.
    pub fn create_seal_commit_task_proto(
        &mut self,
        sector_id: SectorId,
        mode: CommitMode,
    ) -> Result<SealCommitTaskPrototype> {
        let opt_meta = self
            .state
            .staged
            .sectors
            .values_mut()
            .find(|s| s.sector_id == sector_id);

        let meta =
            opt_meta.ok_or_else(|| format_err!("no staged sector with id {} exists", sector_id))?;

        let (ticket, key, pre_commit, seed) = match (mode, &meta.seal_status) {
            (CommitMode::StartFresh(ref s), SealStatus::PreCommitted(t, k, p)) => {
                Ok((t.clone(), k.clone(), p.clone(), s.clone()))
            }
            (CommitMode::StartFresh(_), ss) => Err(format_err!(
                "cannot commit sector with id {:?} and state {:?}",
                sector_id,
                ss,
            )),
            (CommitMode::Resume, SealStatus::CommittingPaused(t, k, p, s)) => {
                Ok((t.clone(), k.clone(), p.clone(), s.clone()))
            }
            (CommitMode::Resume, ss) => Err(format_err!(
                "cannot commit sector with id {:?} and state {:?}",
                sector_id,
                ss,
            )),
        }?;

        // WARNING: Holding this data structure in memory is a stopgap until the SealPreCommitOutput
        // struct is serializable (which requires a working merkle tree cache). This method will
        // panic if you call it for resuming a paused commit after the node is restarted.
        let t_aux = self.temporary_aux.remove(&key).unwrap_or_else(|| {
            panic!("missing pre-commit output for sector with id {}", sector_id)
        });

        let out = Ok(SealCommitTaskPrototype {
            cache_dir: self.sector_store.manager().cache_path(&meta.sector_access),
            piece_info: meta.pieces.iter().map(|x| (x.clone()).into()).collect(),
            porep_config: self.sector_store.proofs_config().porep_config,
            pre_commit: SealPreCommitOutput {
                comm_r: pre_commit.comm_r,
                comm_d: pre_commit.comm_d,
                p_aux: pre_commit.p_aux.clone(),
                t_aux,
            },
            sector_id,
            seed: seed.clone(),
            ticket: ticket.clone(),
        });

        meta.seal_status = SealStatus::Committing(ticket, key, pre_commit, seed);
        self.checkpoint().expects(FATAL_SNPSHT);

        out
    }

    // Produces a vector containing metadata for all staged sectors that this
    // SectorBuilder knows about. If a sealing status is provided, return only
    // the staged sector metadata with matching status.
    pub fn get_staged_sectors_filtered<P: FnMut(&StagedSectorMetadata) -> bool>(
        &self,
        mut predicate: P,
    ) -> Vec<&StagedSectorMetadata> {
        self.state
            .staged
            .sectors
            .values()
            .filter(|x| predicate(*x))
            .collect()
    }

    // Read the raw (without bit-padding) bytes from the provided path into a
    // buffer and return the buffer.
    pub fn read_unsealed_bytes_from(
        &mut self,
        result: Result<(UnpaddedBytesAmount, PathBuf)>,
    ) -> Result<Vec<u8>> {
        result.and_then(|(n, pbuf)| {
            let buffer = self.sector_store.manager().read_raw(
                pbuf.to_str()
                    .ok_or_else(|| format_err!("conversion failed"))?,
                0,
                n,
            )?;

            Ok(buffer)
        })
    }

    // Update metadata to reflect the seal pre-commit result. Propagates the
    // error from the proofs API call if one was present.
    pub fn handle_seal_pre_commit_result(
        &mut self,
        result: SealPreCommitResult,
    ) -> Result<StagedSectorMetadata> {
        let out = {
            let staged_state = &mut self.state.staged;

            let SealPreCommitResult {
                sector_id,
                proofs_api_call_result,
            } = result;

            match proofs_api_call_result {
                Ok(output) => {
                    let SealPreCommitOutput {
                        comm_r,
                        comm_d,
                        p_aux,
                        t_aux,
                    } = output;

                    let meta = staged_state.sectors.get_mut(&sector_id).ok_or_else(|| {
                        format_err!("missing staged sector with id {}", &sector_id)
                    })?;

                    let ticket = meta.seal_status.ticket().ok_or_else(|| {
                        format_err!("failed to get ticket for sector with id {}", sector_id)
                    })?;

                    // TODO: Remove this once we can persist TemporaryAux
                    let _ = self.temporary_aux.insert(TemporaryAuxKey(sector_id), t_aux);

                    meta.seal_status = SealStatus::PreCommitted(
                        ticket.clone(),
                        TemporaryAuxKey(sector_id),
                        PersistablePreCommitOutput {
                            comm_d,
                            comm_r,
                            p_aux,
                        },
                    );

                    Ok(meta.clone())
                }
                Err(err) => {
                    let staged_sector =
                        staged_state.sectors.get_mut(&sector_id).ok_or_else(|| {
                            format_err!("missing staged sector with id {}", &sector_id)
                        })?;

                    staged_sector.seal_status =
                        SealStatus::Failed(format!("seal_pre_commit failed: {:?}", err));

                    Err(err)
                }
            }
        };

        self.checkpoint().expects(FATAL_SNPSHT);

        out
    }

    // Update metadata to reflect the seal commit result. Propagates the error
    // from the proofs API call if one was present, or the appropriate metata
    // object if one was not.
    pub fn handle_seal_commit_result(
        &mut self,
        result: SealCommitResult,
    ) -> Result<SealedSectorMetadata> {
        // scope exists to end the mutable borrow of self so that we can
        // checkpoint
        let out = {
            let SealCommitResult {
                sector_id,
                proofs_api_call_result,
            } = result;

            proofs_api_call_result
                .and_then(|output| {
                    let sector_access = self
                        .sector_store
                        .manager()
                        .convert_sector_id_to_access_name(sector_id)?;

                    let sector_path = self
                        .sector_store
                        .manager()
                        .sealed_sector_path(&sector_access);

                    let staged_sector =
                        self.state.staged.sectors.get(&sector_id).ok_or_else(|| {
                            format_err!("missing staged sector with id {}", &sector_id)
                        })?;

                    let seed = staged_sector.seal_status.seed().ok_or_else(|| {
                        format_err!("failed to get seed for sector with id {}", sector_id)
                    })?;

                    let ticket = staged_sector.seal_status.ticket().ok_or_else(|| {
                        format_err!("failed to get ticket for sector with id {}", sector_id)
                    })?;

                    let pre_commit = staged_sector
                        .seal_status
                        .persistable_pre_commit_output()
                        .ok_or_else(|| {
                            format_err!(
                                "failed to get persistable pre-commit output for sector with id {}",
                                sector_id
                            )
                        })?;

                    let SealCommitOutput { proof } = output;

                    // generate checksum
                    let blake2b_checksum =
                        helpers::calculate_checksum(&sector_path)?.as_ref().to_vec();

                    // get number of bytes in sealed sector-file
                    let len = std::fs::metadata(&sector_path.clone())?.len();

                    // combine the piece commitment, piece inclusion proof, and other piece
                    // metadata into a single struct (to be persisted to metadata store)
                    let pieces = staged_sector.pieces.to_vec();

                    let meta = SealedSectorMetadata {
                        blake2b_checksum,
                        comm_d: pre_commit.comm_d,
                        comm_r: pre_commit.comm_r,
                        len,
                        p_aux: pre_commit.p_aux.clone(),
                        pieces,
                        proof,
                        sector_access,
                        sector_id: staged_sector.sector_id,
                        seed: seed.clone(),
                        ticket: ticket.clone(),
                    };

                    Ok(meta)
                })
                .map_err(|err| {
                    let staged_state = &mut self.state.staged;

                    if let Some(mut staged_sector) = staged_state.sectors.get_mut(&sector_id) {
                        staged_sector.seal_status =
                            SealStatus::Failed(format!("{}", err_unrecov(&err)));
                    }

                    err
                })
                .map(|meta| {
                    self.state.staged.sectors.remove(&sector_id);
                    self.state.sealed.sectors.insert(sector_id, meta.clone());
                    meta
                })
        };

        self.checkpoint().expects(FATAL_SNPSHT);

        out
    }

    // If any sector is full enough to seal, mark it as such.
    fn check_and_schedule(&mut self, seal_all_staged_sectors: bool) {
        let staged_state = &mut self.state.staged;

        let to_be_sealed: HashSet<SectorId> = helpers::get_sectors_ready_for_sealing(
            staged_state,
            self.max_user_bytes_per_staged_sector,
            self.max_num_staged_sectors,
            seal_all_staged_sectors,
        )
        .into_iter()
        .collect();

        for mut v in staged_state.sectors.values_mut() {
            if to_be_sealed.contains(&v.sector_id) {
                v.seal_status = SealStatus::FullyPacked;
            }
        }
    }

    // Create and persist metadata snapshot.
    fn checkpoint(&self) -> Result<()> {
        helpers::persist_snapshot(
            &self.kv_store,
            &SnapshotKey::new(self.prover_id, self.sector_size),
            &self.state,
        )?;

        Ok(())
    }
}
