#[derive(Debug, PartialEq)]
pub enum Error {
    UndefinedTipSet,
    NoBlocks,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Error::UndefinedTipSet => write!(f, "Undefined tipset"),
            Error::NoBlocks => write!(f, "No blocks for tipset"),
        }
    }
}
