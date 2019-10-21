use std::collections::btree_map::BTreeMap;
use std::collections::HashSet;
use std::path::PathBuf;

use filecoin_proofs::error::ExpectWithBacktrace;
use filecoin_proofs::pieces::get_piece_start_byte;
use filecoin_proofs::{PaddedBytesAmount, PrivateReplicaInfo, SealOutput, UnpaddedBytesAmount};
use storage_proofs::sector::SectorId;

use crate::error::Result;
use crate::kv_store::KeyValueStore;
use crate::scheduler::SealResult;
use crate::state::SectorBuilderState;
use crate::worker::{GeneratePoStTaskPrototype, SealTaskPrototype, UnsealTaskPrototype};
use crate::GetSealedSectorResult::WithHealth;
use crate::{
    err_piecenotfound, err_unrecov, GetSealedSectorResult, PieceMetadata, SealStatus,
    SealedSectorMetadata, SecondsSinceEpoch, SectorStore, StagedSectorMetadata,
};
use crate::{helpers, SealTicket};
use helpers::SnapshotKey;
use std::io::Read;

const FATAL_SNPSHT: &str = "could not snapshot";

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
        // "Sealing." If we do have any of those when we start the Scheduler,
        // we should transition them to "Paused" and let the consumers schedule
        // the resume_seal_sector call.
        //
        // For more information, see rust-fil-sector-builder/17.
        for s in state.staged.sectors.values_mut() {
            if let SealStatus::Sealing(ref ticket) = s.seal_status {
                s.seal_status = SealStatus::Paused(ticket.clone())
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
        }
    }
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
            seal_ticket: sealed_sector.seal_ticket.clone(),
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

    // Commits a sector to a given ticket and flips its status to Sealing.
    pub fn commit_sector_to_ticket(&mut self, sector_id: SectorId, seal_ticket: SealTicket) {
        for (k, mut v) in &mut self.state.staged.sectors {
            if sector_id == *k {
                v.seal_status = SealStatus::Sealing(seal_ticket.clone());
                self.checkpoint().expects(FATAL_SNPSHT);

                return;
            }
        }
    }

    // Create a SealTaskPrototype for each staged sector matching the predicate.
    // If a ticket is already associated with the staged sector, use it to
    // create the proto. Otherwise use the provided ticket.
    pub fn create_seal_task_protos<P: FnMut(&StagedSectorMetadata) -> bool>(
        &mut self,
        seal_ticket: SealTicket,
        predicate: P,
    ) -> Result<Vec<SealTaskPrototype>> {
        let mut protos: Vec<SealTaskPrototype> = Default::default();

        for staged_sector in self.get_staged_sectors_filtered(predicate) {
            // Provision a new sealed sector access through the manager.
            let mgr = self.sector_store.manager();

            let sealed_sector_access = mgr
                .new_sealed_sector_access(staged_sector.sector_id)
                .map_err(failure::Error::from)?;

            let sealed_sector_path = mgr.sealed_sector_path(&sealed_sector_access);

            let staged_sector_path = mgr.staged_sector_path(&staged_sector.sector_access);

            let cache_dir = mgr.cache_path(&staged_sector.sector_access);

            let piece_lens = staged_sector
                .pieces
                .iter()
                .map(|p| p.num_bytes)
                .collect::<Vec<UnpaddedBytesAmount>>();

            let seal_ticket = match staged_sector.seal_status {
                SealStatus::Sealing(ref ticket) => ticket.clone(),
                SealStatus::Paused(ref ticket) => ticket.clone(),
                _ => seal_ticket.clone(),
            };

            protos.push(SealTaskPrototype {
                cache_dir,
                piece_lens,
                porep_config: self.sector_store.proofs_config().porep_config,
                seal_ticket,
                sealed_sector_access,
                sealed_sector_path,
                sector_id: staged_sector.sector_id,
                staged_sector_path,
            })
        }

        Ok(protos)
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

    // Update metadata to reflect the sealing results. Propagates the error from
    // the proofs API call if one was present, otherwise maps API call-output
    // to sealed sector metadata.
    pub fn handle_seal_result(&mut self, result: SealResult) -> Result<SealedSectorMetadata> {
        // scope exists to end the mutable borrow of self so that we can
        // checkpoint
        let out = {
            let staged_state = &mut self.state.staged;
            let sealed_state = &mut self.state.sealed;

            let staged_sector = staged_state
                .sectors
                .get_mut(&result.sector_id)
                .expect("missing staged sector");

            let SealResult {
                sector_id,
                sector_access,
                sector_path,
                seal_ticket,
                proofs_api_call_result,
            } = result;

            proofs_api_call_result
                .and_then(|output| {
                    let SealOutput {
                        comm_r,
                        comm_d,
                        p_aux,
                        proof,
                        comm_ps,
                        piece_inclusion_proofs,
                    } = output;

                    // generate checksum
                    let blake2b_checksum =
                        helpers::calculate_checksum(&sector_path)?.as_ref().to_vec();

                    // get number of bytes in sealed sector-file
                    let len = std::fs::metadata(&sector_path.clone())?.len();

                    // combine the piece commitment, piece inclusion proof, and other piece
                    // metadata into a single struct (to be persisted to metadata store)
                    let pieces = staged_sector
                        .clone()
                        .pieces
                        .into_iter()
                        .zip(comm_ps.iter())
                        .zip(piece_inclusion_proofs.into_iter())
                        .map(|((piece, &comm_p), piece_inclusion_proof)| PieceMetadata {
                            piece_key: piece.piece_key,
                            num_bytes: piece.num_bytes,
                            comm_p: Some(comm_p),
                            piece_inclusion_proof: Some(piece_inclusion_proof.into()),
                        })
                        .collect();

                    let meta = SealedSectorMetadata {
                        sector_id: staged_sector.sector_id,
                        sector_access,
                        pieces,
                        p_aux,
                        comm_r,
                        comm_d,
                        proof,
                        blake2b_checksum,
                        len,
                        seal_ticket,
                    };

                    Ok(meta)
                })
                .map_err(|err| {
                    staged_sector.seal_status =
                        SealStatus::Failed(format!("{}", err_unrecov(&err)));
                    err
                })
                .map(|meta| {
                    staged_sector.seal_status = SealStatus::Sealed(Box::new(meta.clone()));
                    sealed_state.sectors.insert(sector_id, meta.clone());
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
                v.seal_status = SealStatus::ReadyForSealing;
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
