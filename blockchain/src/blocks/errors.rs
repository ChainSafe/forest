use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Error {
    UndefinedTipSet,
    NoBlocks,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Error::UndefinedTipSet => write!(f, "Undefined tipset"),
            Error::NoBlocks => write!(f, "No blocks for tipset"),
        }
    }
}
