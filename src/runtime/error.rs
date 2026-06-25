extern crate alloc;
use alloc::string::String;

pub use super::base::StatusError;

#[derive(Debug)]
pub enum RuntimeError {
    Status(StatusError),
    InvalidArgument(String),
}

impl From<StatusError> for RuntimeError {
    fn from(err: StatusError) -> Self {
        RuntimeError::Status(err)
    }
}

impl core::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RuntimeError::Status(err) => write!(f, "IREE runtime error: {}", err),
            RuntimeError::InvalidArgument(msg) => write!(f, "IREE runtime error: {}", msg),
        }
    }
}

impl core::error::Error for RuntimeError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            RuntimeError::Status(err) => Some(err),
            RuntimeError::InvalidArgument(_) => None,
        }
    }
}
