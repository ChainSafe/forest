use std::io;

// Note some DB's may auto close at end of lifetime, should still be handled
// Note key might be vec<u8>
// Note do we need a tokio runtime?
pub trait database {
    fn open(&self) -> Result<(), io::Error>;
    fn close(&self) -> Result<(), io::Error>;
    fn get<K: AsRef<[u8]>>(&self, key: K) -> Result<vec<u8>, io::Error>;
    fn delete<K: AsRef<[u8]>>(&self, key: K) -> Result<(), io:Error>;
    fn put<K, V>(&self, key: K, value: V) -> Result<(), io:Error>
    where 
        K: AsRef<[u8]>,
        V: AsRef<[u8]>;
}
