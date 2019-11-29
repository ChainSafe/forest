use super::errors::Error;

pub trait Cbor {
    fn unmarshall_cbor(bz: &mut [u8]) -> Result<Self, Error>
    where
        Self: Sized;
    fn marshall_cbor(&self) -> Result<Vec<u8>, Error>;
}
