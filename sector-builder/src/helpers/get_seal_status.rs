use crate::builder::SectorId;
use crate::metadata::SealStatus;
use crate::state::{SealedState, StagedState};
use crate::{err_unrecov, error};

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
    use super::*;

    use std::collections::HashMap;

    use crate::metadata::{SealedSectorMetadata, StagedSectorMetadata};
    use crate::state::{SealedState, SectorBuilderState, StagedState};

    fn setup() -> SectorBuilderState {
        let mut staged_sectors: HashMap<SectorId, StagedSectorMetadata> = Default::default();
        let mut sealed_sectors: HashMap<SectorId, SealedSectorMetadata> = Default::default();

        staged_sectors.insert(
            2,
            StagedSectorMetadata {
                sector_id: 2,
                seal_status: SealStatus::Sealing,
                ..Default::default()
            },
        );

        staged_sectors.insert(
            3,
            StagedSectorMetadata {
                sector_id: 3,
                seal_status: SealStatus::Pending,
                ..Default::default()
            },
        );

        sealed_sectors.insert(
            4,
            SealedSectorMetadata {
                sector_id: 4,
                ..Default::default()
            },
        );

        SectorBuilderState {
            prover_id: Default::default(),
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

        let result = get_seal_status(&staged_state, &sealed_state, 1);
        assert!(result.is_err());

        let result = get_seal_status(&staged_state, &sealed_state, 2).unwrap();
        match result {
            SealStatus::Sealing => (),
            _ => panic!("should have been SealStatus::Sealing"),
        }

        let result = get_seal_status(&staged_state, &sealed_state, 3).unwrap();
        match result {
            SealStatus::Pending => (),
            _ => panic!("should have been SealStatus::Pending"),
        }

        let result = get_seal_status(&staged_state, &sealed_state, 4).unwrap();
        match result {
            SealStatus::Sealed(_) => (),
            _ => panic!("should have been SealStatus::Sealed"),
        }
    }
}
