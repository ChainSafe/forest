use self::ErrorNumber as NShim;
use self::SyscallError as EShim;
use fvm2::kernel::SyscallError as E2;
use fvm3::kernel::SyscallError as E3;
use fvm_shared2::error::ErrorNumber as N2;
use fvm_shared3::error::ErrorNumber as N3;
use std::fmt;

#[derive(Debug, Clone)]
pub struct ErrorNumber(ErrorNumberInner);

impl fmt::Display for ErrorNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            ErrorNumberInner::MappedOr3(it) => it.fmt(f),
            ErrorNumberInner::Unknown2(it) => it.fmt(f),
        }
    }
}

#[derive(Debug, Clone)]
enum ErrorNumberInner {
    MappedOr3(N3),
    Unknown2(N2),
}

impl From<N2> for NShim {
    fn from(value: N2) -> Self {
        use ErrorNumberInner::*;
        Self(match value {
            N2::IllegalArgument => MappedOr3(N3::IllegalArgument),
            N2::IllegalOperation => MappedOr3(N3::IllegalOperation),
            N2::LimitExceeded => MappedOr3(N3::LimitExceeded),
            N2::AssertionFailed => MappedOr3(N3::AssertionFailed),
            N2::InsufficientFunds => MappedOr3(N3::InsufficientFunds),
            N2::NotFound => MappedOr3(N3::NotFound),
            N2::InvalidHandle => MappedOr3(N3::InvalidHandle),
            N2::IllegalCid => MappedOr3(N3::IllegalCid),
            N2::IllegalCodec => MappedOr3(N3::IllegalCodec),
            N2::Serialization => MappedOr3(N3::Serialization),
            N2::Forbidden => MappedOr3(N3::Forbidden),
            N2::BufferTooSmall => MappedOr3(N3::BufferTooSmall),
            o => Unknown2(o),
        })
    }
}

impl From<N3> for NShim {
    fn from(value: N3) -> Self {
        Self(ErrorNumberInner::MappedOr3(value))
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("syscall error: {message} (exit_code={number})")]
pub struct SyscallError {
    pub message: String,
    pub number: NShim,
}

impl From<E2> for EShim {
    fn from(value: E2) -> Self {
        let E2(message, number) = value;
        Self {
            message,
            number: number.into(),
        }
    }
}

impl From<E3> for EShim {
    fn from(value: E3) -> Self {
        let E3(message, number) = value;
        Self {
            message,
            number: number.into(),
        }
    }
}
