use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Error {
    UndefinedTipSet(String),
    NoBlocks,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UndefinedTipSet(msg) => write!(f, "Invalid tipset: {}", msg),
            Error::NoBlocks => write!(f, "No blocks for tipset"),
        }
    }
}
