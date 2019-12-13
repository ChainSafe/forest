use encoding::{Cbor, Error as EncodingError};
use std::ops::{Deref, DerefMut};

/// Method number indicator for calling actor methods
#[derive(Default, Clone, PartialEq, Debug)]
pub struct MethodNum(i32); // TODO: add constraints to this

impl MethodNum {
    /// Constructor for new MethodNum
    pub fn new(num: i32) -> Self {
        Self(num)
    }
}

impl From<MethodNum> for i32 {
    fn from(method_num: MethodNum) -> i32 {
        method_num.0
    }
}

/// Base actor send method
pub const METHOD_SEND: isize = 0;
/// Base actor constructor method
pub const METHOD_CONSTRUCTOR: isize = 1;
/// Base actor cron method
pub const METHOD_CRON: isize = 2;

/// Placeholder for non base methods for actors
// TODO revisit on complete spec
pub const METHOD_PLACEHOLDER: isize = 3;

/// Serialized bytes to be used as individual parameters into actor methods
#[derive(Default, Clone, PartialEq, Debug)]
pub struct Serialized {
    bytes: Vec<u8>,
}

impl Deref for Serialized {
    type Target = Vec<u8>;
    fn deref(&self) -> &Self::Target {
        &self.bytes
    }
}

impl Serialized {
    /// Constructor if data is encoded already
    ///
    /// ### Arguments
    /// * `bytes` - vector of bytes to use as serialized data
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
    /// Contructor for encoding Cbor encodable structure
    ///
    /// ### Arguments
    /// * `obj` - Cbor encodable type
    pub fn serialize(obj: impl Cbor) -> Result<Self, EncodingError> {
        Ok(Self {
            bytes: obj.marshal_cbor()?,
        })
    }
    /// Returns serialized bytes
    pub fn bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }
}

/// Method parameters used in Actor execution
#[derive(Default, Clone, PartialEq, Debug)]
pub struct MethodParams {
    params: Vec<Serialized>,
}

impl Deref for MethodParams {
    type Target = Vec<Serialized>;
    fn deref(&self) -> &Self::Target {
        &self.params
    }
}

impl DerefMut for MethodParams {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.params
    }
}
