use super::errors::Error;

pub trait JSON {
    fn unmarshall_json(&mut self, bz: &mut [u8]) -> Result<(), Error>;
    fn marshall_json(&self) -> Result<Vec<u8>, Error>;
}
