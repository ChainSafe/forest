use std::io::Read;
use std::path::PathBuf;

use filecoin_proofs::types::*;

use crate::error::SectorManagerErr;

pub trait SectorConfig: Sync + Send {
    /// returns the number of user-provided bytes that will fit into a sector managed by this store
    fn max_unsealed_bytes_per_sector(&self) -> UnpaddedBytesAmount;

    /// returns the number of bytes in a sealed sector managed by this store
    fn sector_bytes(&self) -> PaddedBytesAmount;
}

pub trait ProofsConfig: Sync + Send {
    /// returns the configuration used when verifying and generating PoReps
    fn post_config(&self) -> PoStConfig;

    /// returns the configuration used when verifying and generating PoSts
    fn porep_config(&self) -> PoRepConfig;
}

pub trait SectorManager: Sync + Send {
    /// produce the path to the file associated with sealed sector access-token
    fn sealed_sector_path(&self, access: &str) -> PathBuf;

    /// produce the path to the file associated with staged sector access-token
    fn staged_sector_path(&self, access: &str) -> PathBuf;

    /// provisions a new sealed sector and reports the corresponding access
    fn new_sealed_sector_access(&self) -> Result<String, SectorManagerErr>;

    /// provisions a new staging sector and reports the corresponding access
    fn new_staging_sector_access(&self) -> Result<String, SectorManagerErr>;

    /// reports the number of bytes written to an unsealed sector
    fn num_unsealed_bytes(&self, access: &str) -> Result<u64, SectorManagerErr>;

    /// sets the number of bytes in an unsealed sector identified by `access`
    fn truncate_unsealed(&self, access: &str, size: u64) -> Result<(), SectorManagerErr>;

    /// writes `data` to the staging sector identified by `access`, incrementally preprocessing `access`
    fn write_and_preprocess(
        &self,
        access: &str,
        data: &mut dyn Read,
    ) -> Result<UnpaddedBytesAmount, SectorManagerErr>;

    fn delete_staging_sector_access(&self, access: &str) -> Result<(), SectorManagerErr>;

    fn read_raw(
        &self,
        access: &str,
        start_offset: u64,
        num_bytes: UnpaddedBytesAmount,
    ) -> Result<Vec<u8>, SectorManagerErr>;
}

pub trait SectorStore: Sync + Send + Sized {
    fn sector_config(&self) -> &SectorConfig;
    fn proofs_config(&self) -> &ProofsConfig;
    fn manager(&self) -> &SectorManager;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{create_dir_all, File};
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::thread;

    use rand::{thread_rng, Rng};
    use tempfile::NamedTempFile;

    use filecoin_proofs::constants::TEST_SECTOR_SIZE;
    use filecoin_proofs::{FrSafe, SealOutput};

    use crate::disk_backed_storage::new_sector_store;

    const TEST_CLASS: SectorClass = SectorClass(
        SectorSize(TEST_SECTOR_SIZE),
        PoRepProofPartitions(2),
        PoStProofPartitions(1),
    );

    struct Harness<S: SectorStore> {
        prover_id: FrSafe,
        seal_output: SealOutput,
        sealed_access: String,
        sector_id: FrSafe,
        store: S,
        unseal_access: String,
        written_contents: Vec<Vec<u8>>,
    }

    #[derive(Debug, Clone, Copy)]
    enum BytesAmount<'a> {
        Max,
        Offset(u64),
        Exact(&'a [u8]),
    }

    fn create_harness(
        sector_class: SectorClass,
        bytes_amts: &[BytesAmount],
    ) -> Harness<impl SectorStore> {
        let store = create_sector_store(sector_class);
        let mgr = store.manager();
        let cfg = store.sector_config();
        let max: u64 = store.sector_config().max_unsealed_bytes_per_sector().into();

        let staged_access = mgr
            .new_staging_sector_access()
            .expect("could not create staging access");

        let sealed_access = mgr
            .new_sealed_sector_access()
            .expect("could not create sealed access");

        let unseal_access = mgr
            .new_sealed_sector_access()
            .expect("could not create unseal access");

        let prover_id = [2; 31];
        let sector_id = [0; 31];

        let mut written_contents: Vec<Vec<u8>> = Default::default();
        for bytes_amt in bytes_amts {
            let contents = match bytes_amt {
                BytesAmount::Exact(bs) => bs.to_vec(),
                BytesAmount::Max => make_random_bytes(max),
                BytesAmount::Offset(m) => make_random_bytes(max - m),
            };

            // write contents to temp file and return mutable handle
            let mut file = {
                let mut file = NamedTempFile::new().expect("could not create named temp file");
                let _ = file.write_all(&contents);
                let _ = file
                    .seek(SeekFrom::Start(0))
                    .expect("failed to seek to beginning of file");
                file
            };

            assert_eq!(
                contents.len(),
                usize::from(
                    mgr.write_and_preprocess(&staged_access, &mut file)
                        .expect("failed to write and preprocess")
                )
            );

            written_contents.push(contents);
        }

        let seal_output = filecoin_proofs::seal(
            PoRepConfig::from(sector_class),
            mgr.staged_sector_path(&staged_access),
            mgr.sealed_sector_path(&sealed_access),
            &prover_id,
            &sector_id,
            &[],
        )
        .expect("failed to seal");

        let SealOutput {
            comm_r,
            comm_d,
            comm_r_star,
            proof,
            comm_ps: _,
            piece_inclusion_proofs: _,
        } = seal_output.clone();

        // valid commitments
        {
            let is_valid = filecoin_proofs::verify_seal(
                PoRepConfig::from(sector_class),
                comm_r,
                comm_d,
                comm_r_star,
                &prover_id,
                &sector_id,
                &proof,
            )
            .expect("failed to run verify_seal");

            assert!(
                is_valid,
                "verification of valid proof failed for sector_class={:?}, bytes_amts={:?}",
                sector_class, bytes_amts
            );
        }

        // unseal the whole thing
        assert_eq!(
            u64::from(UnpaddedBytesAmount::from(PoRepConfig::from(sector_class))),
            u64::from(
                filecoin_proofs::get_unsealed_range(
                    PoRepConfig::from(sector_class),
                    mgr.sealed_sector_path(&sealed_access),
                    mgr.staged_sector_path(&unseal_access),
                    &prover_id,
                    &sector_id,
                    UnpaddedByteIndex(0),
                    cfg.max_unsealed_bytes_per_sector(),
                )
                .expect("failed to unseal")
            )
        );

        Harness {
            prover_id,
            seal_output,
            sealed_access,
            sector_id,
            store,
            unseal_access,
            written_contents,
        }
    }

    fn create_sector_store(sector_class: SectorClass) -> impl SectorStore {
        let staging_path = tempfile::tempdir().unwrap().path().to_owned();
        let sealed_path = tempfile::tempdir().unwrap().path().to_owned();

        create_dir_all(&staging_path).expect("failed to create staging dir");
        create_dir_all(&sealed_path).expect("failed to create sealed dir");

        new_sector_store(
            sector_class,
            sealed_path.to_str().unwrap().to_owned(),
            staging_path.to_str().unwrap().to_owned(),
        )
    }

    fn make_random_bytes(num_bytes_to_make: u64) -> Vec<u8> {
        let mut rng = thread_rng();
        (0..num_bytes_to_make).map(|_| rng.gen()).collect()
    }

    fn seal_verify_aux(sector_class: SectorClass, bytes_amt: BytesAmount) {
        let h = create_harness(sector_class, &vec![bytes_amt]);

        // invalid commitments
        {
            let is_valid = filecoin_proofs::verify_seal(
                h.store.proofs_config().porep_config(),
                h.seal_output.comm_d,
                h.seal_output.comm_r_star,
                h.seal_output.comm_r,
                &h.prover_id,
                &h.sector_id,
                &h.seal_output.proof,
            )
            .expect("failed to run verify_seal");

            // This should always fail, because we've rotated the commitments in
            // the call. Note that comm_d is passed for comm_r and comm_r_star
            // for comm_d.
            assert!(!is_valid, "proof should not be valid");
        }
    }

    fn post_verify_aux(sector_class: SectorClass, bytes_amt: BytesAmount) {
        let mut rng = thread_rng();
        let h = create_harness(sector_class, &vec![bytes_amt]);
        let seal_output = h.seal_output;

        let comm_r = seal_output.comm_r;
        let comm_rs = vec![comm_r, comm_r];
        let challenge_seed = rng.gen();

        let sealed_sector_path = h
            .store
            .manager()
            .sealed_sector_path(&h.sealed_access)
            .to_str()
            .unwrap()
            .to_string();

        let post_output = filecoin_proofs::generate_post(
            h.store.proofs_config().post_config(),
            challenge_seed,
            vec![
                (Some(sealed_sector_path.clone()), comm_r),
                (Some(sealed_sector_path.clone()), comm_r),
            ],
        )
        .expect("PoSt generation failed");

        let result = filecoin_proofs::verify_post(
            h.store.proofs_config().post_config(),
            comm_rs,
            challenge_seed,
            post_output.proofs,
            post_output.faults,
        )
        .expect("failed to run verify_post");

        assert!(result.is_valid, "verification of valid proof failed");
    }

    fn seal_unsealed_roundtrip_aux(sector_class: SectorClass, bytes_amt: BytesAmount) {
        let h = create_harness(sector_class, &vec![bytes_amt]);

        let unsealed_sector_path = h
            .store
            .manager()
            .staged_sector_path(&h.unseal_access)
            .to_str()
            .unwrap()
            .to_string();

        let mut file = File::open(unsealed_sector_path).unwrap();
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).unwrap();

        // test A
        {
            let read_unsealed_buf = h
                .store
                .manager()
                .read_raw(&h.unseal_access, 0, UnpaddedBytesAmount(buf.len() as u64))
                .expect("failed to read_raw a");

            assert_eq!(
                &buf, &read_unsealed_buf,
                "test A contents differed for sector_class={:?}, bytes_amt={:?}",
                sector_class, bytes_amt
            );
        }

        // test B
        {
            let read_unsealed_buf = h
                .store
                .manager()
                .read_raw(
                    &h.unseal_access,
                    1,
                    UnpaddedBytesAmount(buf.len() as u64 - 2),
                )
                .expect("failed to read_raw a");

            assert_eq!(
                &buf[1..buf.len() - 1],
                &read_unsealed_buf[..],
                "test B contents differed for sector_class={:?}, bytes_amt={:?}",
                sector_class,
                bytes_amt
            );
        }

        let byte_padding_amount = match bytes_amt {
            BytesAmount::Exact(bs) => {
                let max: u64 = h
                    .store
                    .sector_config()
                    .max_unsealed_bytes_per_sector()
                    .into();
                max - (bs.len() as u64)
            }
            BytesAmount::Max => 0,
            BytesAmount::Offset(m) => m,
        };

        assert_eq!(
            h.written_contents[0].len(),
            buf.len() - (byte_padding_amount as usize),
            "length of original and unsealed contents differed for sector_class={:?}, bytes_amt={:?}",
            sector_class,
            bytes_amt
        );

        assert_eq!(
            h.written_contents[0][..],
            buf[0..h.written_contents[0].len()],
            "original and unsealed contents differed for sector_class={:?}, bytes_amt={:?}",
            sector_class,
            bytes_amt
        );
    }

    fn seal_unsealed_range_roundtrip_aux(sector_class: SectorClass, bytes_amt: BytesAmount) {
        let h = create_harness(sector_class, &vec![bytes_amt]);

        let offset = 5;
        let range_length = h.written_contents[0].len() as u64 - offset;

        let sealed_sector_path = h
            .store
            .manager()
            .sealed_sector_path(&h.sealed_access)
            .to_str()
            .unwrap()
            .to_string();

        let unsealed_sector_path = h
            .store
            .manager()
            .staged_sector_path(&h.unseal_access)
            .to_str()
            .unwrap()
            .to_string();

        assert_eq!(
            range_length,
            u64::from(
                filecoin_proofs::get_unsealed_range(
                    h.store.proofs_config().porep_config(),
                    &sealed_sector_path,
                    &unsealed_sector_path,
                    &h.prover_id,
                    &h.sector_id,
                    UnpaddedByteIndex(offset),
                    UnpaddedBytesAmount(range_length),
                )
                .expect("failed to unseal")
            )
        );

        let mut file = File::open(&unsealed_sector_path).unwrap();
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).unwrap();

        assert_eq!(
            h.written_contents[0][(offset as usize)..],
            buf[0..(range_length as usize)],
            "original and unsealed range contents differed for sector_class={:?}, bytes_amt={:?}",
            sector_class,
            bytes_amt
        );
    }

    fn write_and_preprocess_overwrites_unaligned_last_bytes_aux(sector_class: SectorClass) {
        // The minimal reproduction for the bug this regression test checks is to write
        // 32 bytes, then 95 bytes.
        // The bytes must sum to 127, since that is the required unsealed sector size.
        // With suitable bytes (.e.g all 255), the bug always occurs when the first chunk is >= 32.
        // It never occurs when the first chunk is < 32.
        // The root problem was that write_and_preprocess was opening in append mode, so seeking backward
        // to overwrite the last, incomplete byte, was not happening.
        let contents_a = [255; 32];
        let contents_b = [255; 95];

        let h = create_harness(
            sector_class,
            &vec![
                BytesAmount::Exact(&contents_a),
                BytesAmount::Exact(&contents_b),
            ],
        );

        let unseal_access = h
            .store
            .manager()
            .new_sealed_sector_access()
            .expect("could not create unseal access");

        let unsealed_sector_path = h
            .store
            .manager()
            .staged_sector_path(&unseal_access)
            .to_str()
            .unwrap()
            .to_string();

        let sealed_sector_path = h
            .store
            .manager()
            .sealed_sector_path(&h.sealed_access)
            .to_str()
            .unwrap()
            .to_string();

        let _ = filecoin_proofs::get_unsealed_range(
            h.store.proofs_config().porep_config(),
            sealed_sector_path,
            unsealed_sector_path.clone(),
            &h.prover_id,
            &h.sector_id,
            UnpaddedByteIndex(0),
            UnpaddedBytesAmount((contents_a.len() + contents_b.len()) as u64),
        )
        .expect("failed to unseal");

        let mut file = File::open(&unsealed_sector_path).unwrap();
        let mut buf_from_file = Vec::new();
        file.read_to_end(&mut buf_from_file).unwrap();

        assert_eq!(
            contents_a.len() + contents_b.len(),
            buf_from_file.len(),
            "length of original and unsealed contents differed for {:?}",
            sector_class
        );

        assert_eq!(
            contents_a[..],
            buf_from_file[0..contents_a.len()],
            "original and unsealed contents differed for {:?}",
            sector_class
        );

        assert_eq!(
            contents_b[..],
            buf_from_file[contents_a.len()..contents_a.len() + contents_b.len()],
            "original and unsealed contents differed for {:?}",
            sector_class
        );
    }

    /*

    TODO: create a way to run these super-slow-by-design tests manually.

    fn seal_verify_live() {
        seal_verify_aux(ConfiguredStore::Live, 0);
        seal_verify_aux(ConfiguredStore::Live, 5);
    }

    fn seal_unsealed_roundtrip_live() {
        seal_unsealed_roundtrip_aux(ConfiguredStore::Live, 0);
        seal_unsealed_roundtrip_aux(ConfiguredStore::Live, 5);
    }

    fn seal_unsealed_range_roundtrip_live() {
        seal_unsealed_range_roundtrip_aux(ConfiguredStore::Live, 0);
        seal_unsealed_range_roundtrip_aux(ConfiguredStore::Live, 5);
    }

    */

    #[test]
    #[ignore] // Slow test – run only when compiled for release.
    fn seal_verify_test() {
        seal_verify_aux(TEST_CLASS, BytesAmount::Max);
        seal_verify_aux(TEST_CLASS, BytesAmount::Offset(5));
    }

    #[test]
    #[ignore] // Slow test – run only when compiled for release.
    fn seal_unsealed_roundtrip_test() {
        seal_unsealed_roundtrip_aux(TEST_CLASS, BytesAmount::Max);
        seal_unsealed_roundtrip_aux(TEST_CLASS, BytesAmount::Offset(5));
    }

    #[test]
    #[ignore] // Slow test – run only when compiled for release.
    fn seal_unsealed_range_roundtrip_test() {
        seal_unsealed_range_roundtrip_aux(TEST_CLASS, BytesAmount::Max);
        seal_unsealed_range_roundtrip_aux(TEST_CLASS, BytesAmount::Offset(5));
    }

    #[test]
    #[ignore] // Slow test – run only when compiled for release.
    fn write_and_preprocess_overwrites_unaligned_last_bytes() {
        write_and_preprocess_overwrites_unaligned_last_bytes_aux(TEST_CLASS);
    }

    #[test]
    #[ignore] // Slow test – run only when compiled for release.
    fn concurrent_seal_unsealed_range_roundtrip_test() {
        let threads = 5;

        let spawned = (0..threads)
            .map(|_| {
                thread::spawn(|| seal_unsealed_range_roundtrip_aux(TEST_CLASS, BytesAmount::Max))
            })
            .collect::<Vec<_>>();

        for thread in spawned {
            thread.join().expect("test thread panicked");
        }
    }

    #[test]
    #[ignore]
    fn post_verify_test() {
        post_verify_aux(TEST_CLASS, BytesAmount::Max);
    }
}
