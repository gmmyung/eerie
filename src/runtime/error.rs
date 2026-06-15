extern crate alloc;
use super::base;
use alloc::string::String;

#[derive(Debug)]
pub enum RuntimeError {
    StatusError(base::StatusError),
    InstanceMismatch(String),
    InvalidArgument(String),
}

impl From<base::StatusError> for RuntimeError {
    fn from(err: base::StatusError) -> Self {
        RuntimeError::StatusError(err)
    }
}

impl core::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RuntimeError::StatusError(err) => write!(f, "IREE runtime error: {}", err),
            RuntimeError::InstanceMismatch(msg) => write!(f, "IREE runtime error: {}", msg),
            RuntimeError::InvalidArgument(msg) => write!(f, "IREE runtime error: {}", msg),
        }
    }
}

impl core::error::Error for RuntimeError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            RuntimeError::StatusError(err) => Some(err),
            RuntimeError::InstanceMismatch(_) | RuntimeError::InvalidArgument(_) => None,
        }
    }
}
