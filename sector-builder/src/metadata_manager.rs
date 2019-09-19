use std::collections::btree_map::BTreeMap;
use std::collections::HashSet;
use std::path::PathBuf;

use filecoin_proofs::error::ExpectWithBacktrace;
use filecoin_proofs::pieces::get_piece_start_byte;
use filecoin_proofs::{PaddedBytesAmount, PrivateReplicaInfo, SealOutput, UnpaddedBytesAmount};
use storage_proofs::sector::SectorId;

use crate::error::Result;
use crate::helpers;
use crate::kv_store::KeyValueStore;
use crate::state::SectorBuilderState;
use crate::worker::{SealTaskPrototype, UnsealTaskPrototype};
use crate::GetSealedSectorResult::WithHealth;
use crate::{
    err_piecenotfound, err_unrecov, GetSealedSectorResult, PieceMetadata, SealStatus,
    SealedSectorMetadata, SecondsSinceEpoch, SectorStore, StagedSectorMetadata,
};
use helpers::SnapshotKey;

const FATAL_SNPSHT: &str = "could not snapshot";

// The SectorBuilderStateManager is the owner of all sector-related metadata.
// It dispatches expensive operations (e.g. unseal and seal) to the sealer
// worker-threads. Other, inexpensive work (or work which needs to be performed
// serially) is handled by the SectorBuilderStateManager itself.
pub struct SectorMetadataManager<T: KeyValueStore, S: SectorStore> {
    pub kv_store: T,
    pub sector_store: S,
    pub state: SectorBuilderState,
    pub max_num_staged_sectors: u8,
    pub max_user_bytes_per_staged_sector: UnpaddedBytesAmount,
    pub prover_id: [u8; 31],
    pub sector_size: PaddedBytesAmount,
}

impl<T: KeyValueStore, S: SectorStore> SectorMetadataManager<T, S> {
    pub fn generate_post(
        &self,
        comm_rs: &[[u8; 32]],
        challenge_seed: &[u8; 32],
        faults: Vec<SectorId>,
    ) -> Result<Vec<u8>> {
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

                let info = if fault_set.contains(&sector.sector_id) {
                    PrivateReplicaInfo::new_faulty(path_str, sector.comm_r)
                } else {
                    PrivateReplicaInfo::new(path_str, sector.comm_r)
                };

                replicas.insert(sector.sector_id, info);
            }
        }

        filecoin_proofs::generate_post(
            self.sector_store.proofs_config().post_config(),
            challenge_seed,
            &replicas,
        )
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
            porep_config: self.sector_store.proofs_config().porep_config(),
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
        })
    }

    // Returns sealing status for the sector with specified id. If no sealed or
    // staged sector exists with the provided id, produce an error.
    pub fn get_seal_status(&self, sector_id: SectorId) -> Result<SealStatus> {
        helpers::get_seal_status(&self.state.staged, &self.state.sealed, sector_id)
    }

    // Write the piece to storage, obtaining the sector id with which the
    // piece-bytes are now associated and a vector of SealTaskPrototypes.
    pub fn add_piece(
        &mut self,
        piece_key: String,
        piece_bytes_amount: u64,
        piece_file: impl std::io::Read,
        store_until: SecondsSinceEpoch,
    ) -> Result<(SectorId, Vec<SealTaskPrototype>)> {
        let destination_sector_id = helpers::add_piece(
            &self.sector_store,
            &mut self.state.staged,
            piece_bytes_amount,
            piece_key,
            piece_file,
            store_until,
        )?;

        let to_seal = self.check_and_schedule(false)?;
        self.checkpoint().expects(FATAL_SNPSHT);

        Ok((destination_sector_id, to_seal))
    }

    // For demo purposes. Schedules sealing of all staged sectors.
    pub fn seal_all_staged_sectors(&mut self) -> Result<Vec<SealTaskPrototype>> {
        let to_seal = self.check_and_schedule(true)?;
        self.checkpoint().expects(FATAL_SNPSHT);

        Ok(to_seal)
    }

    // Produces a vector containing metadata for all sealed sectors that this
    // SectorBuilder knows about. Includes sector health-information on request.
    pub fn get_sealed_sectors(&self, check_health: bool) -> Result<Vec<GetSealedSectorResult>> {
        use rayon::prelude::*;

        let sectors_iter = self.state.sealed.sectors.values().cloned();

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

    // Produces a vector containing metadata for all staged sectors that this
    // SectorBuilder knows about.
    pub fn get_staged_sectors(&self) -> Result<Vec<StagedSectorMetadata>> {
        Ok(self.state.staged.sectors.values().cloned().collect())
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

    // Update metadata to reflect the sealing results.
    pub fn handle_seal_result(
        &mut self,
        sector_id: SectorId,
        sector_access: String,
        sector_path: PathBuf,
        result: Result<SealOutput>,
    ) {
        // scope exists to end the mutable borrow of self so that we can
        // checkpoint
        {
            let staged_state = &mut self.state.staged;
            let sealed_state = &mut self.state.sealed;

            let staged_sector = staged_state
                .sectors
                .get_mut(&sector_id)
                .expect("missing staged sector");

            let _ = result
                .and_then(|output| {
                    let SealOutput {
                        comm_r,
                        comm_r_star,
                        comm_d,
                        proof,
                        comm_ps,
                        piece_inclusion_proofs,
                    } = output;

                    // generate checksum
                    let blake2b_checksum =
                        helpers::calculate_checksum(&sector_path)?.as_ref().to_vec();

                    // get number of bytes in sealed sector-file
                    let len = std::fs::metadata(&sector_path)?.len();

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
                        comm_r_star,
                        comm_r,
                        comm_d,
                        proof,
                        blake2b_checksum,
                        len,
                    };

                    Ok(meta)
                })
                .map_err(|err| {
                    staged_sector.seal_status = SealStatus::Failed(format!("{}", err_unrecov(err)));
                })
                .map(|meta| {
                    sealed_state.sectors.insert(sector_id, meta.clone());
                    staged_sector.seal_status = SealStatus::Sealed(Box::new(meta));
                });
        }

        self.checkpoint().expects(FATAL_SNPSHT);
    }

    // Returns a vector of SealTaskPrototype, each representing a sector which
    // is to be sealed.
    fn check_and_schedule(
        &mut self,
        seal_all_staged_sectors: bool,
    ) -> Result<Vec<SealTaskPrototype>> {
        let staged_state = &mut self.state.staged;

        let to_be_sealed = helpers::get_sectors_ready_for_sealing(
            staged_state,
            self.max_user_bytes_per_staged_sector,
            self.max_num_staged_sectors,
            seal_all_staged_sectors,
        );

        let mut to_seal: Vec<SealTaskPrototype> = Default::default();

        // Mark the to-be-sealed sectors as no longer accepting data and then
        // schedule sealing.
        for sector_id in to_be_sealed {
            let mut staged_sector = staged_state
                .sectors
                .get_mut(&sector_id)
                .ok_or_else(|| err_unrecov(format!("missing sector id={:?}", sector_id)))?;

            // Provision a new sealed sector access through the manager.
            let sealed_sector_access = self
                .sector_store
                .manager()
                .new_sealed_sector_access(staged_sector.sector_id)
                .map_err(failure::Error::from)?;

            let sealed_sector_path = self
                .sector_store
                .manager()
                .sealed_sector_path(&sealed_sector_access);

            let staged_sector_path = self
                .sector_store
                .manager()
                .staged_sector_path(&staged_sector.sector_access);

            let piece_lens = staged_sector
                .pieces
                .iter()
                .map(|p| p.num_bytes)
                .collect::<Vec<UnpaddedBytesAmount>>();

            // mutate staged sector state such that we don't try to write any
            // more pieces to it
            staged_sector.seal_status = SealStatus::Sealing;

            to_seal.push(SealTaskPrototype {
                piece_lens,
                porep_config: self.sector_store.proofs_config().porep_config(),
                sealed_sector_access,
                sealed_sector_path,
                sector_id,
                staged_sector_path,
            });
        }

        Ok(to_seal)
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
