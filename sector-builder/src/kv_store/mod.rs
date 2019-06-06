use std::path::Path;

use crate::error::Result;

mod fs;
mod sled;

pub use self::fs::*;
pub use self::sled::*;

pub trait KeyValueStore: Sized + Sync + Send {
    fn initialize<P: AsRef<Path>>(root_dir: P) -> Result<Self>;
    fn put(&self, key: &[u8], value: &[u8]) -> Result<()>;
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
}
