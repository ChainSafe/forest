use rs_car::{CarDecodeError, Cid};

#[derive(Debug)]
pub enum ReadSingleFileError {
    IoError(std::io::Error),
    CarDecodeError(CarDecodeError),
    NotSingleRoot { roots: Vec<Cid> },
    InvalidUnixFs(String),
    InvalidUnixFsHash(String),
    MissingNode(Cid),
    MaxBufferedData(usize),
    RootCidIsNotFile,
    DataNodesNotSorted,
    PendingLinksAtEOF(Vec<Cid>),
    PBLinkHasNoHash,
    InternalError(String),
}

impl From<CarDecodeError> for ReadSingleFileError {
    fn from(error: CarDecodeError) -> Self {
        match error {
            CarDecodeError::IoError(err) => ReadSingleFileError::IoError(err),
            err => ReadSingleFileError::CarDecodeError(err),
        }
    }
}

impl From<std::io::Error> for ReadSingleFileError {
    fn from(error: std::io::Error) -> Self {
        ReadSingleFileError::IoError(error)
    }
}

impl std::fmt::Display for ReadSingleFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ReadSingleFileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ReadSingleFileError::IoError(err) => Some(err),
            ReadSingleFileError::CarDecodeError(err) => Some(err),
            _ => None,
        }
    }
}
