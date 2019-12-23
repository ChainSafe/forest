use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Error {
    /// Key was not found
    UndefinedKey(String),
    /// Tipset contains no blocks
    NoBlocks,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UndefinedKey(msg) => write!(f, "Invalid key: {}", msg),
            Error::NoBlocks => write!(f, "No blocks for tipset"),
        }
    }
}
