use thiserror::Error;
extern crate alloc;
use super::base;

#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("IREE runtime error: {0}")]
    StatusError(#[from] base::status::StatusError),
    #[error("IREE runtime error: Module index ({0}) out of bounds")]
    OutOfBounds(usize),
    #[error("IREE runtime error: Value type mismatch")]
    ValueTypeMismatch,
}
