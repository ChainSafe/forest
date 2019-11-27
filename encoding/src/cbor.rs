use super::errors::Error;

pub trait Cbor {
    fn unmarshall_cbor(&mut self, bz: &mut [u8]) -> Result<(), Error>;
    fn marshall_cbor(&self) -> Result<Vec<u8>, Error>;
}
