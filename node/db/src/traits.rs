use super::errors::Error;

// Having sizing issues, would rather not Box<Self>
// pub trait DatabaseService {
//     fn open(path: &Path) -> Result<Self, Error>;
// }

pub trait Write {
    fn write(&self, key: Vec<u8>, value: Vec<u8>) -> Result<(), Error>;
    fn delete(&self, key: Vec<u8>) -> Result<(), Error>;
    fn bulk_write(&self, keys: &[Vec<u8>], values: &[Vec<u8>]) -> Result<(), Error>;
    fn bulk_delete(&self, keys: &[Vec<u8>]) -> Result<(), Error>;
}

pub trait Read {
    fn read(&self, key: Vec<u8>) -> Result<Vec<u8>, Error>;
    fn exists(&self, key: Vec<u8>) -> Result<bool, Error>;
    // fn bulk_read(&self, keys: &[Vec<u8>]) -> Result<&[Vec<u8>], Error>;
}
