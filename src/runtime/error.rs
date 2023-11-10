use thiserror::Error;

use super::base;

#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("IREE runtime error: {0}")]
    StatusError(#[from]base::StatusError),
    #[error("IREE runtime error: {0}")]
    InstanceMismatch(String),
}
