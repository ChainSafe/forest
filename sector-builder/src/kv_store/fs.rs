use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use blake2b_simd::State as Blake2b;

use crate::error::Result;
use crate::kv_store::KeyValueStore;

const FATAL_NOCREATE: &str = "[KeyValueStore#put] could not create path";

// FileSystemKvs is a file system-backed key/value store, mostly lifted from
// sile/ekvsb
#[derive(Debug)]
pub struct FileSystemKvs {
    root_dir: PathBuf,
}

impl FileSystemKvs {
    fn key_to_path(&self, key: &[u8]) -> PathBuf {
        let mut hasher = Blake2b::new();
        hasher.update(key);

        let file = hasher.finalize().to_hex();
        self.root_dir.join(&file[..32])
    }
}

impl KeyValueStore for FileSystemKvs {
    fn initialize<P: AsRef<Path>>(root_dir: P) -> Result<Self> {
        fs::create_dir_all(&root_dir)?;

        Ok(FileSystemKvs {
            root_dir: root_dir.as_ref().to_path_buf(),
        })
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        let path = self.key_to_path(key);

        fs::create_dir_all(path.parent().expect(FATAL_NOCREATE))?;

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;

        file.write_all(value)?;

        Ok(())
    }

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let path = self.key_to_path(key);

        match File::open(path) {
            Err(e) => {
                if e.kind() != ErrorKind::NotFound {
                    Err(e)?;
                }
                Ok(None)
            }
            Ok(mut file) => {
                let mut buf = Vec::new();
                file.read_to_end(&mut buf)?;
                Ok(Some(buf))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alpha() {
        let metadata_dir = tempfile::tempdir().unwrap();
        let db = FileSystemKvs::initialize(metadata_dir).unwrap();

        let k_a = b"key-xx";
        let k_b = b"key-yy";
        let v_a = b"value-aa";
        let v_b = b"value-bb";

        db.put(k_a, v_a).unwrap();
        db.put(k_b, v_b).unwrap();

        let opt = db.get(k_a).unwrap();
        assert_eq!(format!("{:x?}", opt.unwrap()), format!("{:x?}", v_a));
    }
}
