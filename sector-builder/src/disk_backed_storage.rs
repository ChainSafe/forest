use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use filecoin_proofs::fr32::target_unpadded_bytes;
use filecoin_proofs::types::*;

use crate::error::SectorManagerErr;
use storage_proofs::sector::SectorId;

// This is a segmented sectorid expression protocol, to support meaningful sector name on disk
// See: https://github.com/filecoin-project/rust-fil-proofs/issues/620 for the details
// Currently, only the default one - on (original) and an IP example design are supported,
// To create a mechanism to support future extension
#[derive(Debug)]
#[allow(dead_code)] // IpV4(String) below is dead code, put it there for reference purpose only
pub enum SectorAccessProto {
    // complicant with the original design, only the lower 32bit is used for sectorId index for a casual miner
    // The sector_access_name is like: on-000000000000-dddddddddd  (on means original)
    Original(u32), // Here the parameter is the segment index, set to 0 by default

    // an example for extension using IP address of NAS storage
    // The sector_access_name is like: ip-192168001010-dddddddddd
    // Here the parameter is IpV4 bytes, e.g. IpV4(192,168,0,10)
    IpV4(u8, u8, u8, u8),
    // Leave for future protocol extension, e.g.
    // Uuid(String, u32),     // to indicate a media with UUID
}

pub struct SectorManager {
    staging_path: PathBuf,
    sealed_path: PathBuf,
    cache_root: PathBuf,

    // A sector ID presentation with a defined protocol
    sector_access_proto: SectorAccessProto,
    sector_segment_id: u32,
}

fn join_root<P: AsRef<Path>>(sector_dir: P, access: &str) -> PathBuf {
    let mut file_path = PathBuf::from(sector_dir.as_ref());
    file_path.push(access);

    file_path
}

impl SectorManager {
    pub fn sealed_sector_path(&self, access: &str) -> PathBuf {
        join_root(&self.sealed_path, access)
    }

    pub fn staged_sector_path(&self, access: &str) -> PathBuf {
        join_root(&self.staging_path, access)
    }

    pub fn cache_path(&self, access: &str) -> PathBuf {
        join_root(&self.cache_root, access)
    }

    pub fn new_sealed_sector_access(
        &self,
        sector_id: SectorId,
    ) -> Result<String, SectorManagerErr> {
        self.new_sector_access(&Path::new(&self.sealed_path), sector_id)
    }

    pub fn new_staging_sector_access(
        &self,
        sector_id: SectorId,
    ) -> Result<String, SectorManagerErr> {
        let access_name = self.convert_sector_id_to_access_name(sector_id)?;

        let mut p: PathBuf = Default::default();
        p.push(&self.cache_root);
        p.push(access_name);

        create_dir_all(&p)
            .map_err(|err| {
                SectorManagerErr::ReceiverError(format!("could not create cache dir: {:?}", err))
            })
            .and_then(|_| self.new_sector_access(&Path::new(&self.staging_path), sector_id))
    }

    pub fn num_unsealed_bytes(&self, access: &str) -> Result<u64, SectorManagerErr> {
        OpenOptions::new()
            .read(true)
            .open(self.staged_sector_path(access))
            .map_err(|err| SectorManagerErr::CallerError(format!("{:?}", err)))
            .map(|mut f| {
                target_unpadded_bytes(&mut f)
                    .map_err(|err| SectorManagerErr::ReceiverError(format!("{:?}", err)))
            })
            .and_then(|n| n)
    }

    pub fn read_raw(
        &self,
        access: &str,
        start_offset: u64,
        num_bytes: UnpaddedBytesAmount,
    ) -> Result<Vec<u8>, SectorManagerErr> {
        OpenOptions::new()
            .read(true)
            .open(self.staged_sector_path(access))
            .map_err(|err| SectorManagerErr::CallerError(format!("{:?}", err)))
            .and_then(|mut file| -> Result<Vec<u8>, SectorManagerErr> {
                file.seek(SeekFrom::Start(start_offset))
                    .map_err(|err| SectorManagerErr::CallerError(format!("{:?}", err)))?;

                let mut buf = vec![0; usize::from(num_bytes)];

                file.read_exact(buf.as_mut_slice())
                    .map_err(|err| SectorManagerErr::CallerError(format!("{:?}", err)))?;

                Ok(buf)
            })
    }

    fn new_sector_access(
        &self,
        root: &Path,
        sector_id: SectorId,
    ) -> Result<String, SectorManagerErr> {
        let access = self.convert_sector_id_to_access_name(sector_id)?;
        let file_path = root.join(&access);

        create_dir_all(root)
            .map_err(|err| {
                SectorManagerErr::ReceiverError(format!(
                    "failed to create sector-holding directory at {:?}: {}",
                    root, err
                ))
            })
            .and_then(|_| {
                File::create(&file_path).map(|_| 0).map_err(|err| {
                    SectorManagerErr::ReceiverError(format!(
                        "failed to create sector file at {:?}: {}",
                        &file_path, err
                    ))
                })
            })
            .map(|_| access)
    }

    pub fn convert_sector_id_to_access_name(
        &self,
        sector_id: SectorId,
    ) -> Result<String, SectorManagerErr> {
        let sector_id = u64::from(sector_id);
        let seg_id = (sector_id >> 32) as u32;
        let index = (sector_id & 0x0000_0000_ffff_ffff) as u32;

        if seg_id != self.sector_segment_id {
            // Strictly check if the sector_segment is the same as the initated one.
            Err(SectorManagerErr::CallerError(format!(
                "seg_id({}) does not match the setting({})",
                seg_id, self.sector_segment_id
            )))
        } else {
            match &self.sector_access_proto {
                SectorAccessProto::Original(_) => Ok(format!("on-{:012}-{:010}", seg_id, index)),
                SectorAccessProto::IpV4(ip1, ip2, ip3, ip4) => Ok(format!(
                    "ip-{:03}{:03}{:03}{:03}-{:010}",
                    ip1, ip2, ip3, ip4, index
                )),
            }
        }
    }

    #[allow(dead_code)]
    fn convert_sector_access_name_to_id(
        &self,
        access_name: &str,
    ) -> Result<SectorId, SectorManagerErr> {
        let ind = self
            .sector_access_proto
            .validate_and_return_index(access_name)?;

        Ok(SectorId::from(
            (u64::from(self.sector_segment_id) << 32) + u64::from(ind),
        ))
    }
}

struct SectorAccessSplit<'a> {
    proto: &'a str,
    seg_str: &'a str,
    ind_str: &'a str,
}

// Some functions below for future use.
#[allow(dead_code)]
impl SectorAccessProto {
    // Check the format is as defined
    fn validate_format<'a>(
        &self,
        access_name: &'a str,
    ) -> Result<SectorAccessSplit<'a>, SectorManagerErr> {
        if access_name.len() != 26
            || access_name.chars().nth(2).unwrap() != '-'
            || access_name.chars().nth(15).unwrap() != '-'
        {
            Err(SectorManagerErr::CallerError(format!(
                "The sector file name '{}' is not supported in this version",
                access_name
            )))
        } else {
            Ok(SectorAccessSplit {
                proto: &access_name[..2],
                seg_str: &access_name[3..15],
                ind_str: &access_name[16..],
            })
        }
    }

    // Return the sector index (the lower 32bit value) or Error when the format is incorrect
    fn validate_and_return_index(&self, access_name: &str) -> Result<u32, SectorManagerErr> {
        let sector_access_split = self.validate_format(access_name)?;

        let index = sector_access_split.ind_str.parse::<u32>().map_err(|_| {
            SectorManagerErr::CallerError(format!(
                "sector index {} is invalid",
                sector_access_split.ind_str
            ))
        })?;

        match self {
            SectorAccessProto::Original(seg_id) => {
                if sector_access_split.proto != "on" {
                    Err(SectorManagerErr::CallerError(
                        format!("The worker is set to Original format sector access only, the file '{}' is not.", access_name))
                    )
                } else if sector_access_split.seg_str != format!("{:012}", seg_id) {
                    Err(SectorManagerErr::CallerError(format!(
                        "The seg_id should be {:12}, the file '{}' is not.",
                        seg_id, access_name
                    )))
                } else {
                    Ok(index)
                }
            }
            SectorAccessProto::IpV4(ip1, ip2, ip3, ip4) => {
                if sector_access_split.proto != "ip" {
                    Err(SectorManagerErr::CallerError(format!(
                        "The worker is set to Ip format sector access only, the file '{}' is not.",
                        access_name
                    )))
                } else if sector_access_split.seg_str
                    != format!("{:03}{:03}{:03}{:03}", ip1, ip2, ip3, ip4)
                {
                    Err(SectorManagerErr::CallerError(format!(
                        "The seg_id should be {:03}{:03}{:03}{:03}, the file '{}' is not.",
                        ip1, ip2, ip3, ip4, access_name
                    )))
                } else {
                    Ok(index)
                }
            }
        }
    }

    // Return SectorID from the access name, no validation to see if the access_name format is defined by the initiated SectorAccessProto
    // This method could be used when sealing is done by one node, but import by another
    fn get_sector_id_from_access_name(
        &self,
        access_name: &str,
    ) -> Result<SectorId, SectorManagerErr> {
        let sector_access_split = self.validate_format(access_name)?;

        let index = sector_access_split.ind_str.parse::<u32>().map_err(|_| {
            SectorManagerErr::CallerError(format!(
                "sector index {} is invalid",
                sector_access_split.ind_str
            ))
        })?;

        if sector_access_split.proto == "on" {
            let seg_id = u64::from(sector_access_split.seg_str.parse::<u32>().map_err(|_| {
                SectorManagerErr::CallerError(format!(
                    "sector index {} is invalid",
                    sector_access_split.ind_str
                ))
            })?);

            Ok(SectorId::from((seg_id << 32) + u64::from(index)))
        } else if sector_access_split.proto == "ip" {
            // This is an IP lead sector access name
            let mut seg_id: u64 = 0;
            let ip1 = sector_access_split.seg_str[..3].parse::<u64>().unwrap();
            seg_id += ip1;
            seg_id <<= 8;
            let ip2 = sector_access_split.seg_str[3..6].parse::<u64>().unwrap();
            seg_id += ip2;
            seg_id <<= 8;
            let ip3 = sector_access_split.seg_str[6..9].parse::<u64>().unwrap();
            seg_id += ip3;
            seg_id <<= 8;
            let ip4 = sector_access_split.seg_str[9..].parse::<u64>().unwrap();
            seg_id += ip4;
            Ok(SectorId::from((seg_id << 32) + u64::from(index)))
        } else {
            Err(SectorManagerErr::CallerError(format!(
                "the access-name proto {} of access_name {} is not supportede.",
                sector_access_split.proto, access_name
            )))
        }
    }
}

pub struct SectorConfig {
    /// the number of user-provided bytes that will fit into a sector
    pub max_unsealed_bytes_per_sector: UnpaddedBytesAmount,
    /// the number of bytes in a sealed sector
    pub sector_bytes: PaddedBytesAmount,
}

pub struct ProofsConfig {
    pub porep_config: PoRepConfig,
    pub post_config: PoStConfig,
}

pub struct SectorStore {
    proofs_config: ProofsConfig,
    sector_config: SectorConfig,
    manager: SectorManager,
}

impl SectorStore {
    pub fn proofs_config(&self) -> &ProofsConfig {
        &self.proofs_config
    }

    pub fn manager(&self) -> &SectorManager {
        &self.manager
    }

    pub fn sector_config(&self) -> &SectorConfig {
        &self.sector_config
    }
}

pub fn new_sector_store<P: AsRef<Path>>(
    sector_class: SectorClass,
    sealed_sector_dir: P,
    staged_sector_dir: P,
    cache_root: P,
) -> SectorStore {
    // By default, support on-000000000000-dddddddddd format
    let default_access_proto = SectorAccessProto::Original(0);

    let manager = SectorManager {
        staging_path: staged_sector_dir.as_ref().to_owned(),
        sealed_path: sealed_sector_dir.as_ref().to_owned(),
        cache_root: cache_root.as_ref().to_owned(),
        sector_access_proto: default_access_proto,
        sector_segment_id: 0u32,
    };

    SectorStore {
        proofs_config: ProofsConfig {
            porep_config: PoRepConfig::from(sector_class),
            post_config: PoStConfig::from(sector_class),
        },
        sector_config: SectorConfig {
            max_unsealed_bytes_per_sector: UnpaddedBytesAmount::from(sector_class.0),
            sector_bytes: PaddedBytesAmount::from(sector_class.0),
        },
        manager,
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    use std::fs::create_dir_all;

    use filecoin_proofs::constants::{SECTOR_SIZE_256_MIB, SECTOR_SIZE_ONE_KIB};
    use filecoin_proofs::types::{PoRepProofPartitions, SectorSize};

    use tempfile::{self};

    fn create_sector_store(sector_class: SectorClass) -> SectorStore {
        let staging_path = tempfile::tempdir().unwrap().path().to_owned();
        let sealed_path = tempfile::tempdir().unwrap().path().to_owned();
        let cache_root = tempfile::tempdir().unwrap().path().to_owned();

        create_dir_all(&staging_path).expect("failed to create staging dir");
        create_dir_all(&sealed_path).expect("failed to create sealed dir");

        new_sector_store(
            sector_class,
            sealed_path.to_str().unwrap().to_owned(),
            staging_path.to_str().unwrap().to_owned(),
            cache_root.to_str().unwrap().to_owned(),
        )
    }

    #[test]
    fn max_unsealed_bytes_per_sector_checks() {
        let xs = vec![
            (
                SectorClass(SectorSize(SECTOR_SIZE_256_MIB), PoRepProofPartitions(2)),
                266338304,
            ),
            (
                SectorClass(SectorSize(SECTOR_SIZE_ONE_KIB), PoRepProofPartitions(2)),
                1016,
            ),
        ];

        for (sector_class, num_bytes) in xs {
            let storage = create_sector_store(sector_class);
            let cfg = storage.sector_config;
            assert_eq!(u64::from(cfg.max_unsealed_bytes_per_sector), num_bytes);
        }
    }

    #[test]
    fn get_sector_id_from_access_original() {
        // Test original design of sector_access.
        let sector_access_proto = &SectorAccessProto::Original(0u32);
        let sector_id = sector_access_proto
            .get_sector_id_from_access_name("on-000000000000-1234567800")
            .unwrap();
        assert_eq!(sector_id, SectorId::from(0x0000000049960278_u64));

        // With a segment ID - Original
        let sector_access_proto = SectorAccessProto::Original(987654u32);
        let sector_id = sector_access_proto
            .get_sector_id_from_access_name("on-000000987654-0000000010")
            .unwrap();
        assert_eq!(sector_id, SectorId::from(0x000f12060000000a_u64));

        // you could get the sector_id value even the segment_id does not match the initiated one.
        let sector_id = sector_access_proto
            .get_sector_id_from_access_name("on-000001987654-0000000010")
            .unwrap();
        assert_eq!(sector_id, SectorId::from(0x001e54460000000a_u64));
    }

    #[test]
    fn get_sector_id_from_access_ipv4() {
        let sector_access_proto = SectorAccessProto::IpV4(192, 168, 001, 010);
        let sector_id = sector_access_proto
            .get_sector_id_from_access_name("ip-192168001010-0000000010")
            .unwrap();
        assert_eq!(sector_id, SectorId::from(0xc0a8010a0000000a_u64));

        // you could get the sector_id value even the segment_id does not match the initiated one.
        let sector_id = sector_access_proto
            .get_sector_id_from_access_name("ip-192168001011-0000000010")
            .unwrap();
        assert_eq!(sector_id, SectorId::from(0xc0a8010b0000000a_u64));
    }

    #[test]
    fn validate_access_proto() {
        let sector_access_proto = &SectorAccessProto::Original(0_u32);
        let index = sector_access_proto
            .validate_and_return_index("on-000000000000-1234567800")
            .unwrap();
        assert_eq!(index, 0x49960278_u32);

        let res = sector_access_proto.validate_and_return_index("on-000000123456-1234567800");
        assert!(res.is_err(), "seg_id is not matched");

        // With a segment ID - Original
        let sector_access_proto = SectorAccessProto::Original(987654u32);
        let index = sector_access_proto
            .validate_and_return_index("on-000000987654-0000000010")
            .unwrap();
        assert_eq!(index, 0x0000000a_u32);

        let res = sector_access_proto.validate_and_return_index("on-000000987123-0000000010");
        assert!(res.is_err(), "seg_id is not matched");

        let sector_access_proto = SectorAccessProto::IpV4(192, 168, 001, 010);
        let index = sector_access_proto
            .validate_and_return_index("ip-192168001010-0000000010")
            .unwrap();
        assert_eq!(index, 0x0000000a_u32);

        let res = sector_access_proto.validate_and_return_index("ip-192168010011-0000000010");
        assert!(res.is_err(), "segment_index is not match");
    }
}
