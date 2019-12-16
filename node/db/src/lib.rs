use std::io;

// Note some DB's may auto close at end of lifetime, should still be handled
// Note key might be vec<u8>
// Note do we need a tokio runtime?
pub trait database {
    fn open(&self) -> Result<(), io::Error>;
    fn close(&self) -> Result<(), io::Error>;
    fn get(&self, key: String) -> Result<vec<u8>, io::Error>;
    fn put(&self, key: String, value: vec<u8>) -> Result<vec<u8>, io:Error>;
    fn delete(&self, key: String) -> Result<vec<u8>, io:Error>;
}
