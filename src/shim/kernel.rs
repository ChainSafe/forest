use self::ErrorNumber as NShim;
use self::SyscallError as EShim;
use fvm2::kernel::SyscallError as E2;
use fvm3::kernel::SyscallError as E3;
use fvm_shared2::error::ErrorNumber as N2;
use fvm_shared3::error::ErrorNumber as N3;
use itertools::Either;
use std::fmt;

macro_rules! error_number {
    ($($variant:ident),* $(,)?) => {
        #[derive(Debug, Clone)]
        #[non_exhaustive]
        pub enum ErrorNumber {
            $($variant,)*
            Unknown(UnknownErrorNumber),
        }

        $(
            static_assertions::const_assert_eq!(N2::$variant as u32, N3::$variant as u32);
        )*

        impl From<N2> for ErrorNumber {
            fn from(value: N2) -> Self {
                match value {
                    $(N2::$variant => Self::$variant,)*
                    u => Self::Unknown(UnknownErrorNumber(Either::Left(u))),
                }
            }
        }

        impl From<N3> for ErrorNumber {
            fn from(value: N3) -> Self {
                match value {
                    $(N3::$variant => Self::$variant,)*
                    u => Self::Unknown(UnknownErrorNumber(Either::Right(u))),
                }
            }
        }

        impl ErrorNumber {
            fn as_unshimmed(&self) -> Either<N2, N3> {
                match self {
                    $(Self::$variant => Either::Right(N3::$variant),)*
                    Self::Unknown(UnknownErrorNumber(u)) => *u,
                }
            }
        }
    };
}

#[derive(Debug, Clone)]
pub struct UnknownErrorNumber(Either<N2, N3>);

error_number! {
    IllegalArgument,
    IllegalOperation,
    LimitExceeded,
    AssertionFailed,
    InsufficientFunds,
    NotFound,
    InvalidHandle,
    IllegalCid,
    IllegalCodec,
    Serialization,
    Forbidden,
    BufferTooSmall,
}

impl fmt::Display for ErrorNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_unshimmed().fmt(f)
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
