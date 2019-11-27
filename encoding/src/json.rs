pub trait JSON {
    fn unmarshall_json(&mut self, bz: &mut [u8]) -> Result<(), String>;
    fn marshall_json(&self) -> Result<Vec<u8>, String>;
}
