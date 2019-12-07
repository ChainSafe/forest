use encoding::{Cbor, Error as EncodingError};

#[derive(Clone)]
pub struct MethodNum(pub i32); // TODO: add constraints to this

/// Base actor send method
pub const METHOD_SEND: isize = 0;
/// Base actor constructor method
pub const METHOD_CONSTRUCTOR: isize = 1;
/// Base actor cron method
pub const METHOD_CRON: isize = 2;

/// Placeholder for non base methods for actors
// TODO revisit on complete spec
pub const METHOD_PLACEHOLDER: isize = 3;

#[derive(Default)]
pub struct MethodParams(pub Vec<u8>); // TODO

impl MethodParams {
    pub fn serialize(obj: impl Cbor) -> Result<Self, EncodingError> {
        Ok(MethodParams(obj.marshal_cbor()?))
    }
}
