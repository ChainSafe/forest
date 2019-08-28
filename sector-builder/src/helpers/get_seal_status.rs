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
        .map(|sealed_sector| SealStatus::Sealed(Box::new(sealed_sector.clone())))
        .or_else(|| {
            staged_state
                .sectors
                .get(&sector_id)
                .and_then(|staged_sector| Some(staged_sector.seal_status.clone()))
        })
        .ok_or_else(|| err_unrecov(format!("no sector with id {} found", sector_id)).into())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::metadata::{SealedSectorMetadata, StagedSectorMetadata};
    use crate::state::{SealedState, SectorBuilderState, StagedState};

    use super::*;

    fn setup() -> SectorBuilderState {
        let mut staged_sectors: HashMap<SectorId, StagedSectorMetadata> = Default::default();
        let mut sealed_sectors: HashMap<SectorId, SealedSectorMetadata> = Default::default();

        staged_sectors.insert(
            SectorId::from(2),
            StagedSectorMetadata {
                sector_id: SectorId::from(2),
                seal_status: SealStatus::Sealing,
                ..Default::default()
            },
        );

        staged_sectors.insert(
            SectorId::from(3),
            StagedSectorMetadata {
                sector_id: SectorId::from(3),
                seal_status: SealStatus::Pending,
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
            staged: StagedState {
                sector_id_nonce: 0,
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
            SealStatus::Sealing => (),
            _ => panic!("should have been SealStatus::Sealing"),
        }

        let result = get_seal_status(&staged_state, &sealed_state, SectorId::from(3)).unwrap();
        match result {
            SealStatus::Pending => (),
            _ => panic!("should have been SealStatus::Pending"),
        }

        let result = get_seal_status(&staged_state, &sealed_state, SectorId::from(4)).unwrap();
        match result {
            SealStatus::Sealed(_) => (),
            _ => panic!("should have been SealStatus::Sealed"),
        }
    }
}
