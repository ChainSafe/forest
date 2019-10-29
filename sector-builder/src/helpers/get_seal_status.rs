use crate::metadata::SealStatus;
use crate::state::{SealedState, StagedState};
use crate::{err_unrecov, error};
use storage_proofs::sector::SectorId;

pub fn get_seal_status(
    staged_state: &StagedState,
    sealed_state: &SealedState,
    sector_id: SectorId,
) -> error::Result<SealStatus> {
    sealed_state
        .sectors
        .get(&sector_id)
        .map(|sealed_sector| SealStatus::Committed(Box::new(sealed_sector.clone())))
        .or_else(|| {
            staged_state
                .sectors
                .get(&sector_id)
                .map(|staged_sector| staged_sector.seal_status.clone())
        })
        .ok_or_else(|| err_unrecov(format!("no sector with id {} found", sector_id)).into())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::metadata::{SealedSectorMetadata, StagedSectorMetadata};
    use crate::state::{SealedState, SectorBuilderState, StagedState};

    use super::*;
    use crate::SealTicket;

    fn setup() -> SectorBuilderState {
        let mut staged_sectors: HashMap<SectorId, StagedSectorMetadata> = Default::default();
        let mut sealed_sectors: HashMap<SectorId, SealedSectorMetadata> = Default::default();

        staged_sectors.insert(
            SectorId::from(2),
            StagedSectorMetadata {
                sector_id: SectorId::from(2),
                seal_status: SealStatus::PreCommitting(SealTicket {
                    block_height: 1,
                    ticket_bytes: [0u8; 32],
                }),
                ..Default::default()
            },
        );

        staged_sectors.insert(
            SectorId::from(3),
            StagedSectorMetadata {
                sector_id: SectorId::from(3),
                seal_status: SealStatus::AcceptingPieces,
                ..Default::default()
            },
        );

        sealed_sectors.insert(
            SectorId::from(4),
            SealedSectorMetadata {
                sector_id: SectorId::from(4),
                ..Default::default()
            },
        );

        SectorBuilderState {
            last_committed_sector_id: 4.into(),
            staged: StagedState {
                sectors: staged_sectors,
            },
            sealed: SealedState {
                sectors: sealed_sectors,
            },
        }
    }

    #[test]
    fn test_alpha() {
        let state = setup();

        let sealed_state = state.sealed;
        let staged_state = state.staged;

        let result = get_seal_status(&staged_state, &sealed_state, SectorId::from(1));
        assert!(result.is_err());

        let result = get_seal_status(&staged_state, &sealed_state, SectorId::from(2)).unwrap();
        match result {
            SealStatus::PreCommitting(_) => (),
            _ => panic!("should have been SealStatus::SealPreCommitting"),
        }

        let result = get_seal_status(&staged_state, &sealed_state, SectorId::from(3)).unwrap();
        match result {
            SealStatus::AcceptingPieces => (),
            _ => panic!("should have been SealStatus::Pending"),
        }

        let result = get_seal_status(&staged_state, &sealed_state, SectorId::from(4)).unwrap();
        match result {
            SealStatus::Committed(_) => (),
            _ => panic!("should have been SealStatus::Sealed"),
        }
    }
}
