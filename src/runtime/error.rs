#[cfg(feature = "std")]
use thiserror::Error;
extern crate alloc;
use super::base;

#[cfg(feature = "std")]
#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("IREE runtime error: {0}")]
    StatusError(#[from]base::StatusError),
    #[error("IREE runtime error: {0}")]
    InstanceMismatch(String),
}

#[cfg(not(feature = "std"))]
pub enum RuntimeError {
    StatusError(base::StatusError),
    InstanceMismatch(alloc::string::String),
}

#[cfg(not(feature = "std"))]
impl From<base::StatusError> for RuntimeError {
    fn from(err: base::StatusError) -> Self {
        RuntimeError::StatusError(err)
    }
}

#[cfg(not(feature = "std"))]
impl core::fmt::Debug for RuntimeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RuntimeError::StatusError(err) => write!(f, "IREE runtime error: {:?}", err),
            RuntimeError::InstanceMismatch(msg) => write!(f, "IREE runtime error: {}", msg),
        }
    }
}
