use std::cmp::Reverse;

use filecoin_proofs::pieces::sum_piece_bytes_with_alignment;
use filecoin_proofs_api::{SectorId, UnpaddedBytesAmount};

use itertools::chain;

use crate::metadata::{SealStatus, StagedSectorMetadata};
use crate::state::StagedState;

pub fn get_sectors_ready_for_sealing(
    staged_state: &StagedState,
    max_num_staged_sectors: u8,
    seal_all_staged_sectors: bool,
) -> Vec<SectorId> {
    let (full, mut not_full): (Vec<&StagedSectorMetadata>, Vec<&StagedSectorMetadata>) =
        staged_state
            .sectors
            .values()
            .filter(|x| x.seal_status == SealStatus::AcceptingPieces)
            .partition(|x| {
                let pieces: Vec<_> = x.pieces.iter().map(|p| p.num_bytes).collect();
                let max_user_bytes =
                    UnpaddedBytesAmount::from(x.registered_seal_proof.sector_size());
                max_user_bytes <= sum_piece_bytes_with_alignment(&pieces)
            });

    not_full.sort_unstable_by_key(|x| Reverse(x.sector_id));

    let num_to_skip = if seal_all_staged_sectors {
        0
    } else {
        max_num_staged_sectors as usize
    };

    chain(full.into_iter(), not_full.into_iter().skip(num_to_skip))
        .map(|x| x.sector_id)
        .collect::<Vec<SectorId>>()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;

    use crate::metadata::{PieceMetadata, StagedSectorMetadata};
    use crate::state::StagedState;
    use crate::SealTicket;

    use filecoin_proofs_api::{RegisteredSealProof, SectorId};

    fn make_meta(
        m: &mut HashMap<SectorId, StagedSectorMetadata>,
        sector_id: SectorId,
        num_bytes: u64,
        accepting_data: bool,
    ) {
        let seal_status = if accepting_data {
            SealStatus::AcceptingPieces
        } else {
            SealStatus::PreCommitting(SealTicket {
                block_height: 1,
                ticket_bytes: [0u8; 32],
            })
        };

        m.insert(
            sector_id,
            StagedSectorMetadata {
                sector_id,
                pieces: if num_bytes > 0 {
                    vec![PieceMetadata {
                        piece_key: format!("{}", sector_id),
                        num_bytes: UnpaddedBytesAmount(num_bytes),
                        comm_p: [0u8; 32],
                    }]
                } else {
                    vec![]
                },
                seal_status,
                registered_seal_proof: RegisteredSealProof::StackedDrg2KiBV1,
                sector_access: Default::default(),
            },
        );
    }

    #[test]
    fn test_seals_all() {
        let mut m: HashMap<SectorId, StagedSectorMetadata> = HashMap::new();

        make_meta(&mut m, SectorId::from(200), 0, true);
        make_meta(&mut m, SectorId::from(201), 0, true);

        let state = StagedState { sectors: m };

        let to_seal: Vec<SectorId> = get_sectors_ready_for_sealing(&state, 10, true)
            .into_iter()
            .collect();

        assert_eq!(vec![SectorId::from(201), SectorId::from(200)], to_seal);
    }

    #[test]
    fn test_seals_full() {
        let mut m: HashMap<SectorId, StagedSectorMetadata> = HashMap::new();

        make_meta(&mut m, SectorId::from(200), 2032, true);
        make_meta(&mut m, SectorId::from(201), 0, true);

        let state = StagedState { sectors: m };

        let to_seal: Vec<SectorId> = get_sectors_ready_for_sealing(&state, 10, false)
            .into_iter()
            .collect();

        assert_eq!(vec![SectorId::from(200)], to_seal);
    }

    #[test]
    fn test_seals_excess() {
        let mut m: HashMap<SectorId, StagedSectorMetadata> = HashMap::new();

        make_meta(&mut m, SectorId::from(200), 0, true);
        make_meta(&mut m, SectorId::from(201), 0, true);
        make_meta(&mut m, SectorId::from(202), 0, true);
        make_meta(&mut m, SectorId::from(203), 0, true);

        let state = StagedState { sectors: m };

        let to_seal: Vec<SectorId> = get_sectors_ready_for_sealing(&state, 2, false)
            .into_iter()
            .collect();

        assert_eq!(vec![SectorId::from(201), SectorId::from(200)], to_seal);
    }

    #[test]
    fn test_noop() {
        let mut m: HashMap<SectorId, StagedSectorMetadata> = HashMap::new();

        make_meta(&mut m, SectorId::from(200), 0, true);
        make_meta(&mut m, SectorId::from(201), 0, true);
        make_meta(&mut m, SectorId::from(202), 0, true);
        make_meta(&mut m, SectorId::from(203), 0, true);

        let state = StagedState { sectors: m };

        let to_seal: Vec<SectorId> = get_sectors_ready_for_sealing(&state, 4, false)
            .into_iter()
            .collect();

        assert_eq!(vec![SectorId::from(0); 0], to_seal);
    }

    #[test]
    fn test_noop_all_being_sealed() {
        let mut m: HashMap<SectorId, StagedSectorMetadata> = HashMap::new();

        make_meta(&mut m, SectorId::from(200), 127, false);
        make_meta(&mut m, SectorId::from(201), 127, false);
        make_meta(&mut m, SectorId::from(202), 127, false);
        make_meta(&mut m, SectorId::from(203), 127, false);

        let state = StagedState { sectors: m };

        let to_seal: Vec<SectorId> = get_sectors_ready_for_sealing(&state, 4, false)
            .into_iter()
            .collect();

        assert_eq!(vec![SectorId::from(0); 0], to_seal);
    }
}
